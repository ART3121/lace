// O splash do Lace Studio (splash.html). Desenha o logo traço a traço e,
// depois, faz um sinal correr pelos dois fios do laço, do começo ao fim de
// cada um, passando por baixo do outro nos cruzamentos.
//
// A janela principal escreve no localStorage, que as duas janelas dividem,
// o que está fazendo e, para a abertura seguinte, as cores, o idioma e a
// versão (src/state/splash.ts); daqui se lê sem permissão nenhuma do Tauri.
// Ela fecha o splash quando aparece.
(() => {
  const STATUS = 'lace-studio:splash:status';
  const LOOK = 'lace-studio:splash:look';
  const BORN = 'lace-studio:splash:born';

  // Quando este splash apareceu: a janela principal conta daqui o tempo
  // mínimo dele na tela.
  try {
    localStorage.setItem(BORN, String(Date.now()));
  } catch {
    // Sem armazenamento, a principal conta desde o começo dela.
  }

  const read = (key) => {
    try {
      return localStorage.getItem(key);
    } catch {
      return null;
    }
  };

  // As cores, o idioma e a versão da última abertura; sem eles, as cores do
  // Atlas (no CSS) e o idioma do sistema.
  let lang = (navigator.language || '').toLowerCase().startsWith('pt') ? 'pt' : 'en';
  try {
    const look = JSON.parse(read(LOOK) || 'null');
    if (look && (look.lang === 'pt' || look.lang === 'en')) lang = look.lang;
    for (const [name, value] of Object.entries((look && look.colors) || {})) {
      document.documentElement.style.setProperty(`--${name}`, value);
    }
    if (look && look.version) document.getElementById('version').textContent = look.version;
  } catch {
    // Um valor estragado fica com as cores padrão.
  }

  // O que a janela principal está fazendo. Cada texto vem com a hora em que
  // foi escrito: o que sobrou da abertura anterior, mais velho que este
  // splash, fica de fora.
  const status = document.getElementById('status');
  status.textContent = lang === 'pt' ? 'Abrindo' : 'Starting';
  const born = Date.now() - 3000;
  const follow = () => {
    try {
      const current = JSON.parse(read(STATUS) || 'null');
      if (current && current.at >= born && current.text && status.textContent !== current.text) status.textContent = current.text;
    } catch {
      // Um valor estragado fica de fora.
    }
  };
  window.addEventListener('storage', (event) => event.key === STATUS && follow());
  window.setInterval(follow, 120);

  if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;

  const svg = document.querySelector('.mark');
  const strands = ['a', 'b'].map((id) =>
    [...svg.querySelectorAll(`.strand[data-strand="${id}"]`)].map((path) => ({ path, length: path.getTotalLength() })),
  );

  // Onde cada trecho começa ao longo do fio, contando o vão por baixo do
  // outro fio: o traço e o sinal passam por ali sem aparecer.
  for (const pieces of strands) {
    let along = 0;
    pieces.forEach((piece, index) => {
      if (index > 0) {
        const end = pieces[index - 1].path.getPointAtLength(pieces[index - 1].length);
        const start = piece.path.getPointAtLength(0);
        along += Math.hypot(start.x - end.x, start.y - end.y);
      }
      piece.start = along;
      along += piece.length;
    });
  }
  const longest = Math.max(...strands.map((pieces) => pieces[pieces.length - 1].start + pieces[pieces.length - 1].length));

  // O desenho: os dois fios ao mesmo tempo, da esquerda para a direita.
  const DRAW_MS = 1300;
  const drawSpeed = longest / DRAW_MS;
  for (const pieces of strands) {
    for (const { path, length, start } of pieces) {
      // Com unidade: o WebKit (macOS e Linux) não aceita número puro aqui.
      path.style.strokeDasharray = `${length}px`;
      path.style.strokeDashoffset = `${length}px`;
      path.animate([{ strokeDashoffset: `${length}px` }, { strokeDashoffset: '0px' }], {
        duration: length / drawSpeed,
        delay: start / drawSpeed,
        easing: 'linear',
        fill: 'forwards',
      });
    }
  }

  // O sinal: um trecho curto e claro que corre por cada fio e some, e volta
  // depois de uma pausa. Cada pedaço do fio tem a sua cópia, que entra quando
  // o sinal chega ao começo dela.
  const DASH = 9;
  const PULSE_SPEED = longest / 1300;
  const REST_MS = 450;
  const period = (longest + DASH) / PULSE_SPEED + REST_MS;
  window.setTimeout(() => {
    for (const pieces of strands) {
      for (const { path, length, start } of pieces) {
        const pulse = path.cloneNode();
        pulse.setAttribute('class', 'pulse');
        pulse.removeAttribute('style');
        pulse.style.strokeDasharray = `${DASH}px ${length + DASH}px`;
        svg.appendChild(pulse);
        const enter = start / PULSE_SPEED / period;
        const leave = Math.min(1, (start + length + DASH) / PULSE_SPEED / period);
        pulse.animate(
          [
            { strokeDashoffset: `${DASH}px`, offset: 0 },
            { strokeDashoffset: `${DASH}px`, offset: enter },
            { strokeDashoffset: `${-length - 1}px`, offset: leave },
            { strokeDashoffset: `${-length - 1}px`, offset: 1 },
          ],
          { duration: period, iterations: Infinity },
        );
      }
    }
  }, DRAW_MS + 250);
})();
