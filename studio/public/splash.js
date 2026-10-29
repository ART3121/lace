// O splash da AURORA IDE (splash.html). O camaleão aparece numa volta: a
// máscara é uma fatia em volta do centro da espiral que cresce de 0 a 360
// graus, a partir do focinho, no sentido em que a espiral corre.
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
  // Azul (no CSS) e o idioma do sistema.
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

  // A volta: o tracejado do traço da máscara, do tamanho da circunferência,
  // entra do começo (o focinho, às 9 horas) até fechar o círculo. No fim, o
  // traço volta a ser inteiro, sem a emenda do tracejado.
  const arc = document.getElementById('sweep-arc');
  const around = 2 * Math.PI * arc.r.baseVal.value;
  // Com unidade: o WebKit (macOS e Linux) não aceita número puro aqui.
  arc.style.strokeDasharray = `${around}px ${around}px`;
  arc.style.strokeDashoffset = `${around}px`;
  const sweep = arc.animate([{ strokeDashoffset: `${around}px` }, { strokeDashoffset: '0px' }], {
    duration: 900,
    delay: 120,
    easing: 'cubic-bezier(0.45, 0, 0.25, 1)',
    fill: 'forwards',
  });
  sweep.onfinish = () => {
    sweep.cancel();
    arc.style.strokeDasharray = 'none';
    arc.style.strokeDashoffset = '0px';
  };
})();
