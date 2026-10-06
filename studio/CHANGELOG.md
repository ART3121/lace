# Mudanças

O formato segue o [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/);
as versões, o [SemVer](https://semver.org/lang/pt-BR/).

## [0.3.0] - 2026-10-06

### Acrescentado

- Testbench em Python, com o cocotb: **Projeto > Novo testbench cocotb
  (Python)**, ou a linguagem Python em **Novo testbench**, cria um `.py` a
  partir do modelo do Core, com a linha `# aurora-toplevel:` do módulo
  testado; um `.py` que já existe entra por **Adicionar arquivos** ou
  arrastado para Testbenches, e os da pasta com `@cocotb.test` aparecem
  entre os não registrados. Com ele como testbench simulado, Wave (F8) e
  Rápida (F9) rodam os testes no Icarus: cada teste aparece no console Wave
  com o resultado, o que falhou vai para o painel Problemas na linha do
  `.py`, e a onda abre mesmo com teste falhando.

### Mudado

- O terminal acompanha o projeto: ao abrir outro, o shell que está rodando
  entra na pasta dele (`Set-Location` no PowerShell, `cd` nos outros).
- A aba Lace saiu do painel inferior. A saída do `lace install` e do
  `lace update` aparece na tela de Ferramentas, que abre quando o comando
  começa; o início de cada fluxo vai para o console da etapa.
- O Verilog (F7) não compila mais os processadores: verifica o Verilog que
  está no disco, e um processador que nunca foi compilado fica de fora, com
  um aviso. O fluxo completo (F5) compila antes de verificar.
- O esquemático não recusa mais um módulo por ter muitas ligações.
- A onda do Icarus sai sempre em FST, e o layout dos processadores vale
  também para ela.

## [0.2.0] - 2026-10-05

A primeira versão com o Studio, junto com a 0.2.0 do Lace.

### Distribuição

- O Studio passa a morar no repositório do Lace, em `studio/`, e a sair no
  instalador dele como o componente `studio`, marcado na instalação
  Recommended e com `lace install studio` para depois. Fica no bundle
  (`toolchain/studio/`) e ganha atalho no menu de aplicativos: o
  `lace-studio.desktop` no Linux, o `Lace Studio.app` em `~/Applications` no
  macOS, o menu Iniciar no Windows.
- Instalado no bundle, o Studio usa o bundle em que está, antes da pasta
  padrão da instalação (origem `bundled`, "bundle do Studio" na tela de
  ferramentas): uma instalação em outra pasta não abre o bundle de outra.

### Acrescentado

- Janela de IDE: menus, barra de ferramentas com os botões da AURORA (C±,
  Verilog, Wave, Rápida, PRISM, Parar), barra de atividades, barra lateral,
  abas, painel inferior e barra de status, com divisões redimensionáveis.
- Projeto: criar, abrir (inclusive arrastando o `.spf`), fechar, recentes,
  reabrir o último projeto com as abas de cada um.
- Explorador com a vista de fontes (módulos com o topo, testbenches com o
  simulado, processadores com fonte, entradas, saídas e gerados, arquivos
  fora do projeto) e a vista de pastas (criar, renomear, lixeira).
- Processadores SAPHO: criar (os campos do Processor Hub da AURORA),
  configurar frequência, clocks e arrays, entradas e saídas.
- Fluxos do Lace com saída ao vivo e cancelamento: build, check (e lint),
  simulação do projeto ou de um processador (Icarus ou Verilator, com prazo),
  síntese, esquemático, abrir a onda no surfer-aurora, fluxo completo (F5).
- Consoles por etapa (C±, ASM, Verilog, Wave, PRISM, Lace) com links
  `arquivo:linha`, painel Problemas e marcadores no editor.
- Terminal de shell com o `lace` no `PATH`.
- Editor Monaco com C± e assembly do SAPHO (gramáticas portadas da AURORA e
  corrigidas pelo léxico do YANC), Verilog, SystemVerilog e JSON;
  sugestões de C±; detecção de mudança no disco.
- Vistas: boas-vindas, esquemático com zoom, estatísticas da síntese,
  relatórios e comparação, ferramentas do Lace (componentes, executáveis,
  hashes, `lace install`, `lace update --check`), processador, preferências.
- Navegador de fluxo no estilo do Vivado, busca nos arquivos, paleta de
  comandos, abrir arquivo pelo nome.
- Português e inglês.
- Temas, em Preferências > Aparência (um cartão com a prévia de cada um) ou
  Exibir > Selecionar tema: Atlas (o padrão, neutro, com o azul do CERN),
  Atlas Branco, Aurora Legacy (as cores da AURORA), Dark Modern, Light
  Modern, Dracula, Gruvbox Dark e Light, Monokai, Nord, One Dark Pro,
  Solarized Dark e Light, Catppuccin Mocha e Tokyo Night, ou "Do sistema".
  Cada tema pinta a interface, o editor (inclusive C± e asm) e os consoles.
  Quem tinha "escuro" ou "claro" gravado passa ao Atlas ou ao Atlas Branco.
- Modo zen (Ctrl+K Z, Exibir > Modo zen): só o editor, em tela cheia e numa
  coluna centralizada. Ctrl+` abre o terminal numa gaveta embaixo do
  editor; um indicador no canto mostra a operação rodando e o resultado.
  Sai com Ctrl+K Z ou Esc duas vezes, e o layout volta como estava. As
  opções ficam em Preferências > Modo zen.
- Atalhos de duas etapas, como no VS Code (Ctrl+K Z), que convivem com os
  do Monaco (Ctrl+K Ctrl+C continua comentando a linha).
- Documentação: guia do usuário, arquitetura, referência do IPC,
  desenvolvimento e paridade com a AURORA.
- Testes do backend, inclusive dos fluxos contra o bundle instalado.
- Hierarquia do design no explorador (Fontes, Hierarquia, Arquivos): a
  árvore de instâncias do design e de cada testbench, elaborada pelo Icarus,
  com a biblioteca SAPHO, os blocos `generate` resolvidos e o topo marcado.
  Atualiza sozinha depois de cada verificação, simulação, build ou síntese
  que passa, e avisa quando um `.v` mudou depois.
- Fontes: o Verilog gerado de cada processador aparece em Módulos e o
  testbench gerado em Testbenches, marcados como gerados. Cada processador
  separa o programa (C± e assembly), as memórias, a simulação (entradas,
  saídas, onda) e os intermediários do YANC.
- Arrastar e soltar no explorador: mover arquivos e pastas na árvore de
  arquivos (o `.spf` acompanha, com o topo e o testbench escolhido), trocar
  um Verilog entre Módulos e Testbenches, soltar arquivos do sistema numa
  pasta (copia) ou numa seção das fontes (copia para o projeto e registra).
- Árvore: setas, Home e End para andar, direita e esquerda para abrir e
  fechar, o arquivo ativo revelado, as pastas abertas lembradas por projeto,
  marcas de topo, simulado e registrado na árvore de arquivos.
- Limpar relatórios (`lace report clean`): todos, ou todos menos os N mais
  novos, com a lista do que sai antes de apagar; apagar um relatório pelo
  menu dele.
- Modo Vim no editor (monaco-vim), com o modo na barra de status e `:w`,
  `:q`, `:wq`, `:x`, `:wa`.
- No Windows, o terminal pode ser o PowerShell ou o Prompt de Comando
  (Preferências > Terminal).
- Editor dividido: até três grupos lado a lado, cada um com as suas abas e
  o seu editor (`Ctrl+\`, "Dividir à direita", "Abrir ao lado" no
  explorador ou Ctrl+Enter na árvore); arrastar abas entre grupos ou para
  um grupo novo, Ctrl+1/2/3 para ir a um grupo; o mesmo arquivo em dois
  grupos é o mesmo texto. Os grupos voltam com o projeto.
- Explorador: um botão no título recolhe a árvore inteira e, de novo,
  expande tudo, nas Fontes, na Hierarquia e na árvore de arquivos.
- A onda de um processador SAPHO abre arrumada como na AURORA: um grupo por
  processador com I/O, a instrução de assembly e a linha do C± de cada
  ciclo, as variáveis do programa e as flags (o `wave_layout` do Core). Vale na
  aba e na janela.
- A onda numa aba: o cliente web do Surfer dentro do Studio, servido por um
  servidor local em `127.0.0.1`. Preferências > Simulação escolhe aba (o
  padrão) ou janela separada; simular de novo recarrega a aba aberta.
- Ferramentas do Lace: **Atualizar para X**, depois de procurar
  atualizações, quando há um Lace mais novo. Confirma e roda o
  `lace update --yes --json` da instalação como operação, com a saída do
  instalador no console Lace; no Windows, abre o assistente. Não pode ser
  cancelada no meio.
- O azul do CERN (Pantone 286 C, #0033A0) nos detalhes: foco, aba e item
  ativos, botão principal, seleção, destino de arrastar.

### Mudado

- O símbolo do Lace passou a ser a torção (dois fios que trocam de posição
  duas vezes) no lugar do octógono entrelaçado: barra de menus,
  boas-vindas, aba Sobre, favicon e ícones do app.
- As marcas do explorador começam com maiúscula: Topo, Simulado, Gerado,
  Compilado, Não compilado (Top, Simulated, Generated, Built, Not built).
- O seletor Icarus/Verilator saiu da barra de ferramentas; o simulador se
  escolhe no menu Fluxo ou nas Preferências.
- Sobre virou uma aba: versões do Studio, do bundle e de cada componente,
  do Tauri e do motor de páginas; as pastas das preferências e do bundle;
  links da documentação; "Copiar informações" para relatar um problema.

- O logo provisório (um L entre dois pinos) deu lugar à marca do Lace na
  barra de menus, nas boas-vindas, no Sobre, no favicon e nos ícones do app.
- Nome de projeto novo só com letras sem acento, números, `_` e `-`,
  começando por letra; o diálogo avisa enquanto se digita.
- As ações do explorador e dos relatórios ficam na linha do título da barra
  lateral.
- A hierarquia, mover arquivos com o `.spf` em dia e a regra de nome de
  projeto passaram para o Core do Lace (`lace_core::hierarchy`,
  `Project::move_path`, `validate_project_name`, com `lace hierarchy` e
  `lace move` na CLI); o Studio só os chama. A hierarquia ganhou o status e
  os erros de cada elaboração, com arquivo e linha.
- Renomear na árvore de arquivos (F2) passa pelo Core, como mover: um `.v`
  registrado continua no `.spf`.
- O Verilog (F7) e o lint seguem o alvo da barra de ferramentas: com um
  processador escolhido, compilam só ele e verificam o Verilog dele com o
  testbench que o YANC gerou (`lace check -p`), como os outros botões já
  faziam. "Verificar este arquivo" continua valendo para o projeto.
- Qualquer Verilog pode ser escolhido como topo, inclusive o `.v` gerado
  pelo build de um processador e os de fora do projeto, menos nome de
  testbench (`tb_<nome>.v`, `<nome>_tb.v`, `tb.v`, a regra do Core). O
  "Definir como topo" está nas Fontes (módulos, gerados, testbenches com
  outro nome, fora do projeto), na árvore de arquivos e na hierarquia, e o
  diálogo Escolher o topo lista todos os candidatos.
- `lace install` e `lace update` que falham mostram no console Lace e no
  aviso a mensagem que a CLI escreveu no JSON, não só o código de saída.
- O resultado de "Procurar atualizações" fica no alto da tela de
  ferramentas, logo abaixo da pasta do bundle.

### Corrigido

As pendências do teste de fogo de 2026-10-04 estão em `docs/PENDENCIAS.md`
(as do Core e da CLI, em `lace/docs/PENDENCIAS.md`).

- Arrastar um `.v` de fora da pasta do projeto para Módulos ou Testbenches
  copiava o arquivo para a raiz, e a cópia divergia do original. Agora
  registra no lugar, como o menu Adicionar; no mesmo repositório git, o
  `.spf` guarda o caminho relativo.
- Os avisos do `.spf` (caminho de outra máquina achado dentro da pasta, topo
  fora da lista, processador com nome que não compila) aparecem no painel
  Problemas enquanto o projeto está aberto.
- O diálogo de processador novo confere o nome com o Core enquanto o
  usuário digita (palavras do C± e do Verilog, módulos da biblioteca SAPHO,
  nomes longos), mostra o motivo ao passar o mouse e mantém o `#NUBITS`
  igual a `#NBMANT + #NBEXPO + 1`.
- A onda de um processador compilado de novo depois da simulação avisa, na
  aba e na janela, que o PC e a linha aparecem como números até simular de
  novo; antes, o assembly e o C± saíam deslocados sem aviso.
- Com o foco na aba da onda, os atalhos do Studio não funcionavam (o
  teclado era do Surfer até clicar fora). O cliente web passa ao Studio as
  teclas de função e as combinações com Ctrl, Alt ou Cmd; letras, setas,
  espaço e as de edição continuam no Surfer.
- Mudar a ordem dos arquivos das fontes (a ordem dos compiladores, que
  decide quem vê cada `define) exigia tirar e adicionar de novo: agora se
  arrasta um sobre outro da mesma seção, ou usa Subir e Descer na lista no
  menu (`lace order` do Lace).
- Os avisos novos do `.spf` (arquivo repetido nas listas, projeto dentro da
  pasta de outro) aparecem traduzidos no painel Problemas.
- Três códigos de erro novos do Core traduzidos: `ambiguous_module`,
  `invalid_parameter`, `operation_in_progress` (uma simulação do Studio e
  outra da CLI no mesmo processador não se quebram mais: a segunda recusa).

- Com muitas abas abertas, a janela inteira deslizava para o lado junto com
  elas (o explorador e os menus sumiam da tela). A coluna da grade da janela
  não cresce mais com o conteúdo, e a aba ativa entra na vista rolando só a
  barra de abas.
- Trocar de aba recarregava a onda do zero e perdia zoom, cursor e sinais
  acrescentados: as abas de onda ficam montadas enquanto existem.
- Fechar o projeto gravava a sessão vazia por cima das abas salvas; ao
  reabrir, só a aba de boas-vindas voltava.
- Abrir outro projeto mantinha os problemas, os marcadores e o "Último:" do
  anterior.
- Na vista Fontes, arquivo de fora da pasta do projeto (o HITS usa
  `../../rtl/`) aparecia com o caminho absoluto cortado; agora vai o nome,
  com a pasta relativa ao lado. O painel Problemas usa a mesma forma.
- 13 códigos de erro saíam crus, em inglês (`invalid_project`,
  `invalid_bundle`, os da aba de onda); todos têm texto agora, inclusive os
  novos `non_ascii_path` e `schematic_too_large` do Lace.
- Passo cancelado aparecia como "falhou ... (cancelled)", e a barra dizia
  "simulação cancelado": o passo diz "parou (a pedido)", e as palavras de
  status não dependem mais do gênero ("Simulação de x: terminou em ...").
- A Hierarquia de um projeto sem arquivo Verilog mostrava a mensagem crua do
  Lace como erro; agora mostra o estado vazio com "Adicionar arquivos
  Verilog".
- Em janela estreita, as abas do painel inferior passavam por baixo dos
  ícones e a barra lateral ocupava quase metade da largura (teto agora de
  40%).
- O PRISM de um módulo grande demais para o esquemático perdia também a
  síntese: agora a síntese e as estatísticas ficam, e o esquemático recusado
  vira aviso (`schematic_error`).

- A janela abria em branco e ficava assim até o JavaScript carregar (1,5 s
  a 4 s com `npm run tauri dev`, enquanto o Vite serve os módulos). Agora
  ela nasce escondida, com fundo escuro, e aparece já desenhada, no tema
  das preferências e com o último projeto aberto.

- A interface relia o projeto em ciclo: o vigia de arquivos avisava também
  quando um arquivo era só aberto para leitura, e reler o projeto abre o
  `.spf`. Agora só mudança (gravar, criar, apagar, renomear) avisa.
- O Surfer fechado ficava como processo zumbi até o Studio sair.
- Botões que davam erro em vez de ficar desabilitados: Lint e Verilator sem
  o Verilator instalado, Onda sem onda gerada, Último relatório sem
  relatórios (agora um aviso).
- Erros benignos do Monaco (promessas canceladas, leitura da área de
  transferência negada) não vão mais para o log como erro.
- O modo do Vim não aparecia na barra de status: o monaco-vim esconde o
  elemento que recebe ao criá-lo e não o mostra de novo.
