// Placa virtual da AURORA: o laço de simulação em volta do modelo do
// Verilator e os periféricos da placa, simulados nos pinos.
//
// A AURORA gera um `board_main.cpp` que instancia o modelo do topo do
// projeto, liga cada porta a um periférico (chaves, botões, LEDs, displays,
// teclado PS/2, tela VGA, UART) e chama `Board::run`. Este arquivo vem
// embutido no core e é gravado ao lado do `board_main.cpp` a cada build.
//
// Conversa com a AURORA:
//   - entrada (stdin): uma ordem por linha, em texto:
//       set <id> <hex>          valor de chaves ou botões apertados (bit 1 = apertado)
//       send <id> <hex> ...     bytes para o teclado PS/2 ou para o RX da UART
//       pause | run | step <ciclos>
//       limit <fator>           0 = o mais rápido possível; 1 = tempo real
//       fps <n>                 no máximo n quadros de vídeo por segundo
//       quit
//     O fim da entrada (a AURORA fechou) também encerra.
//   - saída (o stdout original): mensagens binárias, um byte de tipo, o
//     tamanho em 4 bytes little-endian e o conteúdo:
//       'H' JSON: pronto, com os periféricos
//       'S' JSON: estado (tempo simulado, velocidade, LEDs, bytes da UART)
//       'F' quadro: id (texto, terminado em 0), largura e altura (u16 LE), RGB
//       'X' JSON: fim ($finish do projeto ou ordem quit)
//     O `$display` do projeto vai para o stderr, não para o protocolo.

#pragma once

#include <algorithm>
#include <atomic>
#include <chrono>
#include <condition_variable>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <deque>
#include <functional>
#include <iostream>
#include <memory>
#include <mutex>
#include <sstream>
#include <string>
#include <thread>
#include <vector>

#if defined(_WIN32)
#include <fcntl.h>
#include <io.h>
#define AURORA_DUP _dup
#define AURORA_DUP2 _dup2
#define AURORA_FILENO _fileno
#else
#include <unistd.h>
#define AURORA_DUP dup
#define AURORA_DUP2 dup2
#define AURORA_FILENO fileno
#endif

namespace aurora_board {

// Pinos ----------------------------------------------------------------------

// Uma porta do topo: o membro do modelo do Verilator (CData, SData, IData ou
// QData, conforme a largura) e a largura em bits. Portas de mais de 64 bits
// não são de placa; o gerador as recusa.
struct Pin {
    std::string name;
    int width = 0;
    void* ptr = nullptr;
    int bytes = 0;

    Pin() = default;
    template <typename T>
    Pin(const char* n, int w, T* p) : name(n), width(w), ptr(p), bytes(sizeof(T)) {}

    bool bound() const { return ptr != nullptr; }
    uint64_t mask() const { return width >= 64 ? ~0ULL : ((1ULL << width) - 1); }
    uint64_t get() const {
        switch (bytes) {
        case 1: return *static_cast<uint8_t*>(ptr);
        case 2: return *static_cast<uint16_t*>(ptr);
        case 4: return *static_cast<uint32_t*>(ptr);
        case 8: return *static_cast<uint64_t*>(ptr);
        }
        return 0;
    }
    void set(uint64_t v) {
        v &= mask();
        switch (bytes) {
        case 1: *static_cast<uint8_t*>(ptr) = static_cast<uint8_t>(v); break;
        case 2: *static_cast<uint16_t*>(ptr) = static_cast<uint16_t>(v); break;
        case 4: *static_cast<uint32_t*>(ptr) = static_cast<uint32_t>(v); break;
        case 8: *static_cast<uint64_t*>(ptr) = v; break;
        }
    }
    bool bit(int i) const { return (get() >> i) & 1; }
};

// Saída para a AURORA ----------------------------------------------------------

class Out {
public:
    void open() {
        // O stdout de verdade fica para o protocolo; o descritor 1 passa a
        // ser o stderr, para o $display do projeto não se misturar a ele.
        std::fflush(stdout);
        int fd = AURORA_DUP(AURORA_FILENO(stdout));
        AURORA_DUP2(AURORA_FILENO(stderr), AURORA_FILENO(stdout));
#if defined(_WIN32)
        _setmode(fd, _O_BINARY);
        file_ = _fdopen(fd, "wb");
#else
        file_ = fdopen(fd, "wb");
#endif
    }
    void message(char type, const std::string& body) { message(type, body.data(), body.size()); }
    void message(char type, const void* data, size_t size) {
        if (!file_) return;
        unsigned char head[5] = {static_cast<unsigned char>(type),
            static_cast<unsigned char>(size & 0xff), static_cast<unsigned char>((size >> 8) & 0xff),
            static_cast<unsigned char>((size >> 16) & 0xff), static_cast<unsigned char>((size >> 24) & 0xff)};
        std::fwrite(head, 1, 5, file_);
        if (size) std::fwrite(data, 1, size, file_);
        std::fflush(file_);
    }

private:
    FILE* file_ = nullptr;
};

inline std::string json_string(const std::string& s) {
    std::string out = "\"";
    for (char c : s) {
        if (c == '"' || c == '\\') { out += '\\'; out += c; }
        else if (static_cast<unsigned char>(c) < 0x20) { char b[8]; std::snprintf(b, sizeof b, "\\u%04x", c); out += b; }
        else out += c;
    }
    return out + "\"";
}

// Periféricos ------------------------------------------------------------------

// O tempo é em picossegundos de simulação.
using Time = uint64_t;
constexpr Time PS_PER_S = 1000000000000ULL;

struct Device {
    std::string id;
    explicit Device(std::string i) : id(std::move(i)) {}
    virtual ~Device() = default;
    // O tipo, para o 'H' (o mesmo nome do JSON da placa).
    virtual const char* kind() const = 0;
    // Antes de cada eval: escrever as entradas do projeto que mudam com o tempo.
    virtual void drive(Time) {}
    // Depois de cada eval: ler as saídas do projeto.
    virtual void sample(Time) {}
    // Uma ordem da AURORA para este periférico (`set` ou `send`).
    virtual void command(const std::string&, std::istringstream&) {}
    // Um pedaço do JSON do estado ("\"id\":...") ou vazio; `window` é o tempo
    // simulado desde o último estado.
    virtual std::string report(Time, Time) { return ""; }
    // Um quadro de vídeo pronto para mandar, se houver.
    virtual bool frame(std::vector<unsigned char>&) { return false; }
};

inline uint64_t parse_hex(const std::string& s) { return std::strtoull(s.c_str(), nullptr, 16); }

// Chaves (bit 1 = para cima) e botões (bit 1 = apertado; `active_low` inverte
// no pino, como os KEY das placas da Terasic).
struct Inputs : Device {
    Pin pin;
    bool active_low;
    bool buttons;
    uint64_t value = 0;
    Inputs(std::string i, Pin p, bool low, bool is_buttons)
        : Device(std::move(i)), pin(std::move(p)), active_low(low), buttons(is_buttons) { apply(); }
    const char* kind() const override { return buttons ? "buttons" : "switches"; }
    void apply() { pin.set(active_low ? ~value : value); }
    void command(const std::string& verb, std::istringstream& in) override {
        if (verb != "set") return;
        std::string v; in >> v;
        value = parse_hex(v) & pin.mask();
        apply();
    }
};

// LEDs e segmentos: quanto tempo cada bit ficou aceso na janela, de 0 a 255,
// para o PWM aparecer como brilho e o display multiplexado como persistência.
struct Lights : Device {
    Pin pin;
    bool active_low;
    const char* kind_;
    uint64_t last = 0;
    Time since = 0;
    std::vector<Time> lit;
    Lights(std::string i, Pin p, bool low, const char* k)
        : Device(std::move(i)), pin(std::move(p)), active_low(low), kind_(k), lit(pin.width, 0) {}
    const char* kind() const override { return kind_; }
    uint64_t value() const { return (active_low ? ~pin.get() : pin.get()) & pin.mask(); }
    void account(Time now) {
        if (now <= since) return;
        Time span = now - since;
        for (int b = 0; b < pin.width; b++) if ((last >> b) & 1) lit[b] += span;
        since = now;
    }
    void sample(Time now) override {
        uint64_t v = value();
        if (v != last) { account(now); last = v; }
    }
    std::string report(Time now, Time window) override {
        account(now);
        std::string out = json_string(id) + ":[";
        for (int b = 0; b < pin.width; b++) {
            unsigned level = window ? static_cast<unsigned>((lit[b] * 255 + window / 2) / window) : (((last >> b) & 1) ? 255 : 0);
            if (level > 255) level = 255;
            out += (b ? "," : "") + std::to_string(level);
            lit[b] = 0;
        }
        return out + "]";
    }
};

// Teclado PS/2, do lado do teclado: cada byte vira um quadro de 11 bits
// (início, 8 bits do menos significativo para o mais, paridade ímpar, fim)
// com o relógio do teclado em tempo simulado. O dado muda com o relógio em
// 1 e o projeto lê na descida. Nada volta do projeto para o teclado.
struct Ps2Keyboard : Device {
    Pin clk, dat;
    Time half;        // meio período do relógio do teclado
    std::deque<uint8_t> queue;
    int bit = -1;     // -1: parado
    int phase = 0;    // 0: dado posto, relógio em 1; 1: relógio em 0
    Time next = 0;
    uint16_t frame = 0;
    Ps2Keyboard(std::string i, Pin c, Pin d, double khz)
        : Device(std::move(i)), clk(std::move(c)), dat(std::move(d)),
          half(static_cast<Time>(PS_PER_S / (khz * 1000.0) / 2)) { clk.set(1); dat.set(1); }
    const char* kind() const override { return "ps2"; }
    void command(const std::string& verb, std::istringstream& in) override {
        if (verb != "send") return;
        std::string b;
        while (in >> b) queue.push_back(static_cast<uint8_t>(parse_hex(b)));
    }
    void drive(Time now) override {
        if (bit < 0) {
            if (queue.empty() || now < next) return;
            uint8_t data = queue.front(); queue.pop_front();
            int ones = 0; for (int k = 0; k < 8; k++) ones += (data >> k) & 1;
            frame = static_cast<uint16_t>((1u << 10) | ((ones % 2 == 0) ? (1u << 9) : 0u) | (static_cast<unsigned>(data) << 1));
            bit = 0; phase = 0; next = now;
        }
        if (now < next) return;
        if (phase == 0) {
            dat.set((frame >> bit) & 1);
            clk.set(1);
            phase = 1; next = now + half / 2;          // o dado se firma antes da descida
        } else if (phase == 1) {
            clk.set(0);
            phase = 2; next = now + half;
        } else {
            clk.set(1);
            bit++;
            phase = 0; next = now + half / 2;
            if (bit > 10) { bit = -1; dat.set(1); next = now + 2 * half; }
        }
    }
};

// UART: o TX do projeto decodificado e o RX gerado, na velocidade dada.
struct Uart : Device {
    Pin tx, rx;
    Time bit_time;
    // RX (da AURORA para o projeto)
    std::deque<uint8_t> queue;
    int rx_bit = -1; Time rx_next = 0; uint8_t rx_byte = 0;
    // TX (do projeto para a AURORA)
    bool tx_last = true; int tx_bit = -1; Time tx_next = 0; uint8_t tx_byte = 0;
    std::vector<uint8_t> received;
    Uart(std::string i, Pin t, Pin r, double baud)
        : Device(std::move(i)), tx(std::move(t)), rx(std::move(r)),
          bit_time(static_cast<Time>(PS_PER_S / baud)) { if (rx.bound()) rx.set(1); }
    const char* kind() const override { return "uart"; }
    void command(const std::string& verb, std::istringstream& in) override {
        if (verb != "send") return;
        std::string b;
        while (in >> b) queue.push_back(static_cast<uint8_t>(parse_hex(b)));
    }
    void drive(Time now) override {
        if (!rx.bound()) return;
        if (rx_bit < 0) {
            if (queue.empty()) return;
            rx_byte = queue.front(); queue.pop_front();
            rx_bit = 0; rx_next = now;
        }
        if (now < rx_next) return;
        if (rx_bit == 0) rx.set(0);
        else if (rx_bit <= 8) rx.set((rx_byte >> (rx_bit - 1)) & 1);
        else rx.set(1);
        rx_next = now + bit_time;
        if (++rx_bit > 9) rx_bit = -1;
    }
    void sample(Time now) override {
        if (!tx.bound()) return;
        bool level = tx.get() & 1;
        if (tx_bit < 0) {
            if (tx_last && !level) { tx_bit = 0; tx_byte = 0; tx_next = now + bit_time + bit_time / 2; }
        } else if (now >= tx_next) {
            if (tx_bit < 8) { tx_byte |= static_cast<uint8_t>(level) << tx_bit; tx_bit++; tx_next += bit_time; }
            else { if (level) received.push_back(tx_byte); tx_bit = -1; }
        }
        tx_last = level;
    }
    std::string report(Time, Time) override {
        if (received.empty()) return "";
        std::string out = json_string(id) + ":[";
        for (size_t k = 0; k < received.size(); k++) out += (k ? "," : "") + std::to_string(received[k]);
        received.clear();
        return out + "]";
    }
};

// Tela VGA (ou o vídeo paralelo do transmissor HDMI): a imagem remontada
// pelos sinais de sincronismo. Amostra uma vez por pixel: na subida do
// relógio de vídeo do projeto, se a placa tem um (VGA_CLK), ou na subida do
// relógio da placa, com o divisor descoberto pela duração da linha. Com o
// sinal de vídeo ativo (BLANK_N, DE), a área visível vem dele; sem ele, da
// tabela de modos VESA.
struct Vga : Device {
    Pin r, g, b, hs, vs, de, pclk, base;
    int width = 0, height = 0;
    struct Mode { int htotal, hactive, hfront, hsync, hback, vtotal, vactive, vfront, vsync, vback; };
    static const Mode* modes(size_t& n) {
        static const Mode table[] = {
            {800, 640, 16, 96, 48, 525, 480, 10, 2, 33},
            {1056, 800, 40, 128, 88, 628, 600, 1, 4, 23},
            {1344, 1024, 24, 136, 160, 806, 768, 3, 6, 29},
            {1650, 1280, 110, 40, 220, 750, 720, 5, 5, 20},
            {2200, 1920, 88, 44, 148, 1125, 1080, 4, 5, 36},
        };
        n = sizeof table / sizeof table[0];
        return table;
    }
    bool last_clk = false, last_hs = true, last_vs = true;
    // Uma linha de amostras: cor e ativo.
    std::vector<uint32_t> line;
    std::vector<uint8_t> line_de;
    std::vector<std::vector<uint32_t>> lines;   // as linhas do quadro em andamento
    std::vector<unsigned char> ready;           // o último quadro completo, RGB
    int ready_w = 0, ready_h = 0;
    bool fresh = false;
    bool hs_low = true, vs_low = true;          // polaridade do pulso (descoberta)
    int hs_low_count = 0;
    std::vector<int> line_lengths;              // amostras por linha, para o divisor

    Vga(std::string i, Pin r_, Pin g_, Pin b_, Pin hs_, Pin vs_, Pin de_, Pin pclk_, Pin base_)
        : Device(std::move(i)), r(std::move(r_)), g(std::move(g_)), b(std::move(b_)), hs(std::move(hs_)),
          vs(std::move(vs_)), de(std::move(de_)), pclk(std::move(pclk_)), base(std::move(base_)) {}
    const char* kind() const override { return "vga"; }

    static unsigned scale(uint64_t v, int w) {
        if (w <= 0) return 0;
        if (w >= 8) return static_cast<unsigned>((v >> (w - 8)) & 0xff);
        uint64_t max = (1ULL << w) - 1;
        return static_cast<unsigned>((v * 255 + max / 2) / max);
    }
    void sample(Time) override {
        const Pin& clock = pclk.bound() ? pclk : base;
        bool c = clock.get() & 1;
        bool rising = c && !last_clk;
        last_clk = c;
        if (!rising) return;
        bool h = hs.get() & 1, v = vs.get() & 1;
        // O pulso de sincronismo é o nível mais curto: com hs em 0 mais da
        // metade da linha, a polaridade é a positiva.
        if (!h) hs_low_count++;
        bool h_pulse_start = hs_low ? (last_hs && !h) : (!last_hs && h);
        bool v_pulse_start = vs_low ? (last_vs && !v) : (!last_vs && v);
        last_hs = h; last_vs = v;
        if (h_pulse_start) end_line();
        if (v_pulse_start) end_frame();
        uint32_t rgb = (scale(r.get(), r.width) << 16) | (scale(g.get(), g.width) << 8) | scale(b.get(), b.width);
        line.push_back(rgb);
        line_de.push_back(de.bound() ? static_cast<uint8_t>(de.get() & 1) : 1);
    }
    void end_line() {
        if (!line.empty()) {
            if (static_cast<int>(line.size()) > 0 && hs_low_count * 2 > static_cast<int>(line.size())) hs_low = !hs_low;
            line_lengths.push_back(static_cast<int>(line.size()));
            if (de.bound()) {
                std::vector<uint32_t> visible;
                for (size_t k = 0; k < line.size(); k++) if (line_de[k]) visible.push_back(line[k]);
                if (!visible.empty()) lines.push_back(std::move(visible));
            } else {
                lines.push_back(line);
            }
        }
        line.clear(); line_de.clear(); hs_low_count = 0;
    }
    // O divisor: quantas amostras por pixel. Com DE, pela largura visível;
    // sem DE, pela linha inteira contra a tabela de modos.
    int divisor_for(int samples, bool visible, const Mode** found) const {
        size_t n; const Mode* table = modes(n);
        for (int d = 1; d <= 4; d++)
            for (size_t m = 0; m < n; m++) {
                int target = (visible ? table[m].hactive : table[m].htotal) * d;
                if (std::abs(samples - target) <= d) { if (found) *found = &table[m]; return d; }
            }
        return 0;
    }
    void end_frame() {
        end_line();
        if (lines.empty()) return;
        std::vector<std::vector<uint32_t>> rows;
        if (de.bound()) {
            int samples = static_cast<int>(lines[0].size());
            int d = divisor_for(samples, true, nullptr);
            if (d == 0) d = 1;
            for (auto& l : lines) {
                std::vector<uint32_t> px;
                for (size_t k = 0; k < l.size(); k += d) px.push_back(l[k]);
                rows.push_back(std::move(px));
            }
        } else {
            // Sem DE: a linha começa no pulso; pula pulso e back porch e
            // pega a área ativa do modo achado.
            std::vector<int> lens = line_lengths;
            std::sort(lens.begin(), lens.end());
            int typical = lens[lens.size() / 2];
            const Mode* mode = nullptr;
            int d = divisor_for(typical, false, &mode);
            if (!mode) { lines.clear(); line_lengths.clear(); return; }
            // O quadro começa no pulso vertical: pula pulso e back porch.
            int first = mode->vsync + mode->vback;
            for (int y = first; y < first + mode->vactive && y < static_cast<int>(lines.size()); y++) {
                std::vector<uint32_t> px;
                int start = (mode->hsync + mode->hback) * d;
                for (int x = 0; x < mode->hactive; x++) {
                    size_t k = static_cast<size_t>(start + x * d);
                    px.push_back(k < lines[y].size() ? lines[y][k] : 0);
                }
                rows.push_back(std::move(px));
            }
        }
        lines.clear(); line_lengths.clear();
        if (rows.empty()) return;
        int w = 0; for (auto& row : rows) w = std::max(w, static_cast<int>(row.size()));
        int h = static_cast<int>(rows.size());
        if (w <= 0 || w > 4096 || h > 4096) return;
        std::vector<unsigned char> rgb(static_cast<size_t>(w) * h * 3, 0);
        for (int y = 0; y < h; y++)
            for (size_t x = 0; x < rows[y].size(); x++) {
                uint32_t p = rows[y][x];
                unsigned char* o = &rgb[(static_cast<size_t>(y) * w + x) * 3];
                o[0] = (p >> 16) & 0xff; o[1] = (p >> 8) & 0xff; o[2] = p & 0xff;
            }
        if (w == ready_w && h == ready_h && rgb == ready) return;   // nada mudou
        ready = std::move(rgb); ready_w = w; ready_h = h; fresh = true;
    }
    bool frame(std::vector<unsigned char>& out) override {
        if (!fresh) return false;
        fresh = false;
        out.clear();
        out.insert(out.end(), id.begin(), id.end());
        out.push_back(0);
        out.push_back(ready_w & 0xff); out.push_back((ready_w >> 8) & 0xff);
        out.push_back(ready_h & 0xff); out.push_back((ready_h >> 8) & 0xff);
        out.insert(out.end(), ready.begin(), ready.end());
        return true;
    }
};

// A placa ------------------------------------------------------------------------

struct Clock {
    Pin pin;
    Time half;
    Time next = 0;
};

class Board {
public:
    void clock(Pin p, double hz) {
        Clock c{std::move(p), static_cast<Time>(PS_PER_S / hz / 2), 0};
        c.next = c.half;
        clocks_.push_back(std::move(c));
    }
    // O relógio principal (o primeiro), para quem amostra nele.
    Pin main_clock() const { return clocks_.empty() ? Pin() : clocks_[0].pin; }
    void add(Device* d) { devices_.emplace_back(d); }

    int run(const std::function<void()>& eval, const std::function<bool()>& finished) {
        out_.open();
        std::thread reader([this] { read_commands(); });
        reader.detach();
        hello();
        for (auto& c : clocks_) c.pin.set(0);
        for (auto& d : devices_) d->drive(0);
        eval();
        Time now = 0, last_report_t = 0;
        uint64_t cycles = 0;
        auto wall0 = std::chrono::steady_clock::now();
        auto last_report = wall0, last_frame = wall0;
        Time limit_base_t = 0; auto limit_base_wall = wall0;
        std::string reason = "quit";
        while (true) {
            if ((cycles & 1023) == 0) {
                if (!handle_commands(now, cycles)) break;
                if (paused_ && !wait_while_paused(now, cycles)) break;
                auto wall = std::chrono::steady_clock::now();
                if (limit_changed_) { limit_changed_ = false; limit_base_t = now; limit_base_wall = wall; }
                if (limit_ > 0) {
                    double sim = static_cast<double>(now - limit_base_t) / PS_PER_S;
                    double real = std::chrono::duration<double>(wall - limit_base_wall).count();
                    double ahead = sim / limit_ - real;
                    if (ahead > 0.002) std::this_thread::sleep_for(std::chrono::duration<double>(ahead));
                }
                double since_report = std::chrono::duration<double>(wall - last_report).count();
                if (since_report >= 1.0 / 30) {
                    report(now, now - last_report_t, cycles, since_report);
                    last_report = wall; last_report_t = now; report_cycles_ = cycles;
                }
                double since_frame = std::chrono::duration<double>(wall - last_frame).count();
                if (max_fps_ > 0 && since_frame >= 1.0 / max_fps_) {
                    std::vector<unsigned char> f;
                    for (auto& d : devices_) if (d->frame(f)) { out_.message('F', f.data(), f.size()); last_frame = wall; }
                }
            }
            // O próximo flanco de relógio.
            Time t = ~0ULL;
            for (auto& c : clocks_) t = std::min(t, c.next);
            if (clocks_.empty()) t = now + 1000;
            now = t;
            for (auto& c : clocks_)
                if (c.next == now) { c.pin.set(c.pin.get() ^ 1); c.next += c.half; }
            for (auto& d : devices_) d->drive(now);
            eval();
            for (auto& d : devices_) d->sample(now);
            cycles++;
            if (step_left_ > 0 && --step_left_ == 0) paused_ = true;
            if (finished()) { reason = "finish"; break; }
        }
        report(now, now - last_report_t, cycles, 0);
        std::vector<unsigned char> f;
        for (auto& d : devices_) if (d->frame(f)) out_.message('F', f.data(), f.size());
        out_.message('X', "{\"reason\":" + json_string(reason) + ",\"time_ps\":" + std::to_string(now) +
                              ",\"evals\":" + std::to_string(cycles) + "}");
        return 0;
    }

private:
    std::vector<Clock> clocks_;
    std::vector<std::unique_ptr<Device>> devices_;
    Out out_;
    std::mutex mutex_;
    std::condition_variable wake_;
    std::deque<std::string> commands_;
    std::atomic<bool> pending_{false};
    bool closed_ = false;
    bool paused_ = false;
    uint64_t step_left_ = 0;
    double limit_ = 0;
    bool limit_changed_ = false;
    double max_fps_ = 30;
    uint64_t report_cycles_ = 0;

    void hello() {
        std::string s = "{\"version\":1,\"clocks\":[";
        for (size_t k = 0; k < clocks_.size(); k++)
            s += (k ? "," : "") + std::string("{\"pin\":") + json_string(clocks_[k].pin.name) +
                 ",\"half_ps\":" + std::to_string(clocks_[k].half) + "}";
        s += "],\"devices\":[";
        for (size_t k = 0; k < devices_.size(); k++)
            s += (k ? "," : "") + std::string("{\"id\":") + json_string(devices_[k]->id) +
                 ",\"kind\":" + json_string(devices_[k]->kind()) + "}";
        out_.message('H', s + "]}");
    }

    void read_commands() {
        std::string line;
        while (std::getline(std::cin, line)) {
            if (!line.empty() && line.back() == '\r') line.pop_back();
            std::lock_guard<std::mutex> lock(mutex_);
            commands_.push_back(line);
            pending_ = true;
            wake_.notify_all();
        }
        std::lock_guard<std::mutex> lock(mutex_);
        closed_ = true;
        pending_ = true;
        wake_.notify_all();
    }

    // Aplica as ordens que chegaram; falso para encerrar.
    bool handle_commands(Time, uint64_t) {
        if (!pending_) return true;
        std::deque<std::string> batch;
        bool closed;
        {
            std::lock_guard<std::mutex> lock(mutex_);
            batch.swap(commands_);
            pending_ = false;
            closed = closed_;
        }
        for (auto& line : batch) {
            std::istringstream in(line);
            std::string verb;
            in >> verb;
            if (verb == "quit") return false;
            if (verb == "pause") paused_ = true;
            else if (verb == "run") { paused_ = false; step_left_ = 0; limit_changed_ = true; }
            else if (verb == "step") { uint64_t n = 0; in >> n; step_left_ = n ? n : 1; paused_ = false; }
            else if (verb == "limit") { in >> limit_; limit_changed_ = true; }
            else if (verb == "fps") { in >> max_fps_; }
            else if (verb == "set" || verb == "send") {
                std::string id; in >> id;
                for (auto& d : devices_) if (d->id == id) d->command(verb, in);
            }
        }
        return !(closed && batch.empty()) || !closed ? !closed || !batch.empty() : false;
    }

    bool wait_while_paused(Time now, uint64_t cycles) {
        report(now, 0, cycles, 0);
        while (paused_) {
            {
                std::unique_lock<std::mutex> lock(mutex_);
                wake_.wait(lock, [this] { return pending_.load(); });
            }
            if (!handle_commands(now, cycles)) return false;
            if (closed_) return false;
        }
        limit_changed_ = true;
        return true;
    }

    void report(Time now, Time window, uint64_t cycles, double wall_window) {
        std::string s = "{\"time_ps\":" + std::to_string(now) + ",\"evals\":" + std::to_string(cycles);
        double speed = 0;
        if (wall_window > 0) speed = (static_cast<double>(window) / PS_PER_S) / wall_window;
        char buf[64];
        std::snprintf(buf, sizeof buf, ",\"speed\":%.6g", speed);
        s += buf;
        s += std::string(",\"paused\":") + (paused_ ? "true" : "false");
        std::string lights, uart;
        for (auto& d : devices_) {
            std::string part = d->report(now, window);
            if (part.empty()) continue;
            std::string& into = std::strcmp(d->kind(), "uart") == 0 ? uart : lights;
            into += (into.empty() ? "" : ",") + part;
        }
        s += ",\"lights\":{" + lights + "},\"uart\":{" + uart + "}}";
        out_.message('S', s);
    }
};

}  // namespace aurora_board
