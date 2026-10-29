//! A cara da AURORA no terminal: as cores da marca e a marca desenhada em
//! braille, das mesmas linhas do `studio/public/brand/lace-icon.svg`.

use std::sync::OnceLock;

use ratatui::style::Color;

/// As linhas da marca, no quadro de 64 por 64 do SVG: dois fios que se
/// cruzam, com o vão de cada cruzamento já nos traços.
const MARK: [&str; 4] = [
    "M4 19 L12.6 19.06 L13.5 19.36 L14.4 19.91 L15.3 20.71 L16.2 21.73 L17.1 22.95 L18 24.36 L18.9 25.92 L19.8 27.6 L20.7 29.36 L21.6 31.18 L22.5 33.02 L23.4 34.84 L24.3 36.6 L25.2 38.26 L26.1 39.81 L27 41.19 L27.9 42.4 L28.8 43.39 L29.7 44.16 L30.6 44.69 L31.5 44.96 L32.4 44.97 L33.3 44.73 L34.2 44.23 L35.1 43.49 L36 42.52 L36.9 41.34 L37.2 40.9",
    "M46.8 23.1 L47.7 21.85 L48.6 20.81 L49.5 19.99 L50.4 19.41 L51.3 19.08 L52 19 L60 19",
    "M4 45 L12.6 44.94 L13.5 44.64 L14.4 44.09 L15.3 43.29 L16.2 42.27 L17.1 41.05 L17.1 41.05",
    "M26.7 23.25 L27.6 21.98 L28.5 20.92 L29.4 20.07 L30.3 19.46 L31.2 19.1 L32.1 19 L33 19.16 L33.9 19.57 L34.8 20.24 L35.7 21.13 L36.6 22.25 L37.5 23.56 L38.4 25.03 L39.3 26.65 L40.2 28.37 L41.1 30.17 L42 32 L42.9 33.83 L43.8 35.63 L44.7 37.35 L45.6 38.97 L46.5 40.44 L47.4 41.75 L48.3 42.87 L49.2 43.76 L50.1 44.43 L51 44.84 L51.9 45 L60 45",
];

/// A faixa do quadro que os fios ocupam (x de 4 a 60, y de 19 a 45).
const X0: f64 = 4.0;
const X1: f64 = 60.0;
const Y0: f64 = 19.0;
const Y1: f64 = 45.0;

/// O terminal mostra cor de 24 bits? (`COLORTERM=truecolor` ou `24bit`.)
/// Sem isso, as cores da marca vão para as mais próximas da paleta de 256.
fn truecolor() -> bool {
    static TRUECOLOR: OnceLock<bool> = OnceLock::new();
    *TRUECOLOR.get_or_init(|| {
        std::env::var("COLORTERM")
            .map(|v| matches!(v.as_str(), "truecolor" | "24bit"))
            .unwrap_or(false)
    })
}

/// O azul claro da marca sobre fundo escuro (`#9bb0e8`, o dos fios na
/// versão invertida): títulos, o item escolhido, a barra de progresso.
pub fn accent() -> Color {
    if truecolor() {
        Color::Rgb(0x9b, 0xb0, 0xe8)
    } else {
        Color::Indexed(147)
    }
}

/// O creme da marca (`#f4efe6`): o nome da AURORA ao lado da marca.
pub fn cream() -> Color {
    if truecolor() {
        Color::Rgb(0xf4, 0xef, 0xe6)
    } else {
        Color::Indexed(230)
    }
}

/// A marca em braille, com `cols` colunas de largura (cada caractere tem
/// 2 por 4 pontos). Os fios saem com dois pontos de espessura.
pub fn mark(cols: usize) -> Vec<String> {
    let width = cols * 2;
    let scale = (width - 1) as f64 / (X1 - X0);
    let height = ((Y1 - Y0) * scale).ceil() as usize + 1;
    let rows = height.div_ceil(4);
    let mut dots = vec![vec![false; width]; rows * 4];
    let radius = 0.8;
    for path in MARK {
        let points = polyline(path);
        for pair in points.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let length = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
            let steps = (length * scale * 4.0).ceil().max(1.0) as usize;
            for s in 0..=steps {
                let t = s as f64 / steps as f64;
                let x = (a.0 + (b.0 - a.0) * t - X0) * scale;
                let y = (a.1 + (b.1 - a.1) * t - Y0) * scale + 0.5;
                for (row, line) in dots.iter_mut().enumerate() {
                    for (col, dot) in line.iter_mut().enumerate() {
                        let (dx, dy) = (col as f64 - x, row as f64 - y);
                        if dx * dx + dy * dy <= radius * radius {
                            *dot = true;
                        }
                    }
                }
            }
        }
    }
    // Os pontos de cada caractere braille, na ordem dos bits do Unicode.
    const BITS: [(usize, usize, u32); 8] = [
        (0, 0, 0x01),
        (0, 1, 0x02),
        (0, 2, 0x04),
        (1, 0, 0x08),
        (1, 1, 0x10),
        (1, 2, 0x20),
        (0, 3, 0x40),
        (1, 3, 0x80),
    ];
    (0..rows)
        .map(|r| {
            (0..cols)
                .map(|c| {
                    let bits = BITS
                        .iter()
                        .filter(|(dx, dy, _)| dots[r * 4 + dy][c * 2 + dx])
                        .fold(0, |acc, (_, _, bit)| acc | bit);
                    char::from_u32(0x2800 + bits).unwrap_or(' ')
                })
                .collect::<String>()
                .trim_end_matches('\u{2800}')
                .to_owned()
        })
        .collect()
}

/// Os pontos de um caminho só com `M` e `L`.
fn polyline(path: &str) -> Vec<(f64, f64)> {
    path.split(['M', 'L'])
        .filter_map(|p| {
            let mut xy = p.split_whitespace().map(|n| n.parse::<f64>());
            match (xy.next(), xy.next()) {
                (Some(Ok(x)), Some(Ok(y))) => Some((x, y)),
                _ => None,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mark_has_the_width_asked_and_both_wires() {
        let lines = mark(24);
        assert!((4..=6).contains(&lines.len()), "{lines:#?}");
        assert!(lines.iter().all(|l| l.chars().count() <= 24));
        // Os fios entram pela esquerda em cima e embaixo, e saem pela
        // direita nas mesmas alturas.
        let first = lines.first().unwrap();
        let last = lines.last().unwrap();
        assert!(first.starts_with(|c: char| c != '\u{2800}'), "{lines:#?}");
        assert!(last.starts_with(|c: char| c != '\u{2800}'), "{lines:#?}");
        assert_eq!(first.chars().count(), 24, "{lines:#?}");
        assert_eq!(last.chars().count(), 24, "{lines:#?}");
    }

    #[test]
    fn polylines_read_every_point() {
        assert_eq!(polyline(MARK[1]).len(), 8);
        assert_eq!(polyline("M1 2 L3 4.5"), [(1.0, 2.0), (3.0, 4.5)]);
    }
}
