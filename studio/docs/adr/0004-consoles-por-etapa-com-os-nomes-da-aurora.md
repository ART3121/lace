# 0004. Consoles xterm.js por etapa, com os nomes da AURORA

- **Status:** Aceita
- **Data:** 2026-10-03

## Contexto

A AURORA mostrava a saída de cada etapa num terminal próprio (TCMM, TASM,
TVERI, TWAVE, TPRISM) e tinha um shell de verdade (TCMD). Quem vem dela
procura a saída do compilador no mesmo lugar. Uma simulação pode escrever
dezenas de milhares de linhas; uma lista de elementos no DOM não aguenta
isso.

## Decisão

- Um xterm.js somente leitura por etapa, com os nomes da AURORA sem o T
  (C±, ASM, Verilog, Wave, PRISM), e um console Lace para o que o próprio
  Studio faz. Cada linha vai para o console do passo do Core que a escreveu.
- Os consoles vivem fora do React: escrever não redesenha a interface, e a
  saída não se perde ao trocar de aba.
- O backend junta os eventos do Core em lotes (até 500 ou 30 ms) antes de
  mandar para a interface.
- O terminal de shell é um xterm.js ligado a um pseudoterminal do backend
  (`portable-pty`), com o shell do usuário e o `lace` no `PATH`.

## Consequências

- O painel troca sozinho para o console da etapa que está rodando, como a
  AURORA, menos quando o usuário está no terminal de shell.
- O teste de hardware (THTEST) não tem console porque o Lace ainda não o
  implementa.
