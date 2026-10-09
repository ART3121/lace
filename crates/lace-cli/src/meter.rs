//! A linha de progresso dos downloads do `lace install` e do `lace update`:
//! o que está vindo, a barra, a porcentagem e os MiB, numa linha do stderr
//! que se reescreve no máximo quatro vezes por segundo.
//!
//! Com o stderr fora de um terminal (um pipe, o Studio, o log do CI), cada
//! item vira uma linha só, sem barra. A linha cabe na largura do terminal:
//! uma linha quebrada não volta ao começo com o `\r`.

use std::io::{IsTerminal, Write};
use std::time::{Duration, Instant};

/// Intervalo mínimo entre dois desenhos.
const FRAME: Duration = Duration::from_millis(250);

pub struct Meter {
    tty: bool,
    label: String,
    total: u64,
    done: u64,
    drawn: Option<Instant>,
    width: usize,
}

impl Meter {
    pub fn new() -> Meter {
        Meter {
            tty: std::io::stderr().is_terminal(),
            label: String::new(),
            total: 0,
            done: 0,
            drawn: None,
            width: 0,
        }
    }

    /// Começa um item: `label` (`[2/3] Lace Studio`), com `total` bytes a
    /// baixar (zero: desconhecido). Termina o anterior, se havia.
    pub fn start(&mut self, label: String, total: u64) {
        self.finish();
        self.label = label;
        self.total = total;
        self.done = 0;
        if self.tty {
            self.draw();
        } else if total > 0 {
            eprintln!("  {} ({})", self.label, lace_installer::mib(total));
        } else {
            eprintln!("  {}", self.label);
        }
    }

    /// `done` bytes do item atual já vieram.
    pub fn bytes(&mut self, done: u64) {
        self.done = done;
        if self.tty && self.drawn.is_none_or(|t| t.elapsed() >= FRAME) {
            self.draw();
        }
    }

    /// Uma linha que não é do download (`Verifying the executables`).
    pub fn note(&mut self, text: &str) {
        self.finish();
        eprintln!("  {text}");
    }

    /// Fecha a linha do item atual.
    pub fn finish(&mut self) {
        if self.label.is_empty() {
            return;
        }
        if self.tty {
            if self.total > 0 {
                self.done = self.total;
            }
            self.draw();
            eprintln!();
        }
        self.label.clear();
        self.drawn = None;
        self.width = 0;
    }

    fn draw(&mut self) {
        let columns = ratatui::crossterm::terminal::size().map_or(80, |(c, _)| c as usize);
        let line = line(&self.label, self.done, self.total, columns.max(20) - 1);
        let pad = self.width.saturating_sub(line.chars().count());
        let mut err = std::io::stderr().lock();
        let _ = write!(err, "\r{line}{}", " ".repeat(pad));
        let _ = err.flush();
        self.width = line.chars().count();
        self.drawn = Some(Instant::now());
    }
}

impl Drop for Meter {
    fn drop(&mut self) {
        self.finish();
    }
}

/// A linha de um item em até `room` colunas: o rótulo, a barra (de 10 a 30
/// colunas), a porcentagem e os MiB; o rótulo é cortado se não couber.
fn line(label: &str, done: u64, total: u64, room: usize) -> String {
    let mib = |b: u64| b as f64 / 1_048_576.0;
    if total == 0 {
        return fit(&format!("  {label}  {:.1} MiB", mib(done)), room);
    }
    let fraction = (done as f64 / total as f64).min(1.0);
    let numbers = format!(
        " {:>3}%  {:.1}/{:.1} MiB",
        (fraction * 100.0).floor(),
        mib(done),
        mib(total)
    );
    let label_width = label.chars().count().min(32);
    let free = room.saturating_sub(2 + label_width + 2 + 2 + numbers.len());
    let bar = free.clamp(10, 30);
    let filled = (fraction * bar as f64).floor() as usize;
    let text = format!(
        "  {}  [{}{}]{numbers}",
        fit(label, label_width),
        "#".repeat(filled),
        ".".repeat(bar - filled)
    );
    fit(&text, room)
}

/// `text` em até `room` caracteres.
fn fit(text: &str, room: usize) -> String {
    text.chars().take(room).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_line_fits_the_terminal() {
        for room in [39, 59, 79, 119] {
            for done in [0, 3_000_000, 14_000_000] {
                let text = line("[2/3] Lace Studio", done, 14_000_000, room);
                assert!(text.chars().count() <= room, "{room}: {text}");
                assert!(text.contains('['), "{room}: {text}");
            }
        }
        let text = line("[2/3] Lace Studio", 7_000_000, 14_000_000, 79);
        assert!(text.contains(" 50%"), "{text}");
        assert!(text.contains("6.7/13.4 MiB"), "{text}");
        assert_eq!(
            line("[1/1] Lace", 1_048_576, 0, 79),
            "  [1/1] Lace  1.0 MiB"
        );
    }
}
