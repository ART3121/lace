# Paridade com a AURORA

O Lace Studio substitui a AURORA e precisa fazer tudo o que ela fazia. Esta
página lista cada recurso da AURORA, levantado do código dela
(`lace/vendor/aurora`), e diz onde ele está no Studio ou por que ainda não
está. Atualize a linha no mesmo commit que mudar o estado de um recurso.

| Estado | Significa |
|---|---|
| **Feito** | funciona no Studio 0.1 |
| **Parcial** | parte funciona; a nota diz o que falta |
| **Fase 2**, **Fase 3** | planejado para a fase indicada (ver o fim da página) |
| **Depende do Lace** | precisa de algo que o `lace-core` ainda não tem; a nota diz o quê |
| **Diferente** | o Studio faz de outro jeito, por decisão do Lace ou do Studio |

## Projeto

| Recurso da AURORA | Estado | No Studio, ou o que falta |
|---|---|---|
| Criar projeto (nome e pasta) | Feito | Arquivo > Novo projeto (Ctrl+Alt+N) |
| Abrir `.spf` | Feito | Arquivo > Abrir projeto (Ctrl+Shift+O), arrastar o `.spf` para a janela, recentes |
| Fechar projeto | Feito | Arquivo > Fechar projeto; pergunta pelos arquivos não salvos |
| Reabrir o último projeto e as abas | Feito | preferência "Reabrir o último projeto"; as abas de cada projeto voltam como estavam |
| Mudança no disco por fora | Feito | aba sem alteração recarrega sozinha; com alteração, salvar pergunta (sobrescrever, recarregar, cancelar) |
| Recentes, com os ausentes riscados e "esquecer" | Feito | tela inicial e Arquivo > Abrir recente |
| Procurar no disco um recente que sumiu | Fase 2 | |
| Apagar projeto (lixeira) | Fase 2 | |
| Backup do projeto em zip | Fase 2 | |
| Renomear projeto | Depende do Lace | o Core não tem operação de renomear projeto |
| Instalar os projetos de exemplo | Fase 2 | os exemplos do Lace (`contador`, `soma`, `com_erro`) |
| Aviso de arquivos registrados que sumiram | Fase 2 | |

## Processadores SAPHO

| Recurso da AURORA | Estado | No Studio, ou o que falta |
|---|---|---|
| Processor Hub: nome, linguagem, bits, ganho, mantissa, expoente, pilhas, portas | Feito | Projeto > Novo processador (Ctrl+Alt+P); padrões do Core, iguais aos da AURORA |
| Fonte C± ou C (`.cmm`, `.cpp`) | Feito | |
| Frequência, número de clocks, arrays na onda, tempo estimado | Feito | aba do processador (duplo clique nele no explorador) |
| Arquivos `input_<n>.txt` e `output_<n>.txt` | Feito | criar entrada, abrir, e os valores de cada saída depois da simulação |
| "Processador ativo" pelo arquivo focado | Diferente | o alvo é escolhido na barra de ferramentas (projeto ou um processador) e lembrado por projeto |
| Renomear ou apagar processador | Depende do Lace | o Core cria e configura processador, mas não renomeia nem remove |
| Nome com `-` | Diferente | o Lace recusa, porque o `cmmcomp` não aceita no `#PRNAME` (API.md do Lace, seção 10) |

## Explorador

| Recurso da AURORA | Estado | No Studio, ou o que falta |
|---|---|---|
| Vista de arquivos Verilog com selos de topo e testbench | Feito | Explorador > Fontes: Módulos (topo marcado com o módulo, e o Verilog gerado de cada processador), Testbenches (o simulado marcado, e o testbench gerado de cada processador), Processadores (programa, memórias, simulação, intermediários do YANC), Fora do projeto |
| Classificação sintetizável ou testbench pelo conteúdo | Feito | pela regra da AURORA, no Core (`add_verilog`) |
| Definir topo, marcar testbench, novo `.v`, menu de contexto | Feito | qualquer `.v` pode ser topo, inclusive o gerado pelo build de um processador; nome de testbench (`tb_<nome>.v`, `<nome>_tb.v`) não |
| Arrastar arquivos para importar | Feito | do sistema: numa pasta da árvore de arquivos, copia; em Módulos ou Testbenches, copia para o projeto e registra com esse papel; `.spf` abre o projeto. Dentro da árvore: mover arquivos e pastas (o `.spf` acompanha) e trocar um Verilog de Módulos para Testbenches |
| Vista de pastas: novo arquivo e pasta, renomear (F2), apagar para a lixeira, copiar caminho, mostrar no gerenciador | Feito | Explorador > Arquivos |
| Recortar, copiar e colar arquivos; desfazer apagar | Fase 2 | |
| Vista de hierarquia | Diferente | Explorador > Hierarquia. A AURORA montava a hierarquia com o Yosys; o Studio usa a elaboração do Icarus, que atualiza sozinha depois de cada verificação, simulação, build ou síntese e mostra também cada testbench e a biblioteca SAPHO |
| Decorações do git na árvore | Fase 3 | junto com o painel de Git |

## Compilar, verificar, simular, sintetizar

| Botão da AURORA | Estado | No Studio |
|---|---|---|
| C± (F6) | Feito | C± (F6): `build` dos processadores, ou só o alvo |
| Verilog (F7) | Feito | Verilog (F7): `check`, que roda só o Icarus e não compila os processadores (a AURORA compila); Shift+F7 acrescenta o lint do Verilator |
| Wave (F8) | Feito | Wave (F8): simula e abre a onda no surfer-aurora |
| Fast Sim (F9) | Feito | Rápida (F9): simula sem abrir a onda |
| PRISM (F10) | Diferente | PRISM (F10): síntese do Yosys e esquemático do `show` + Graphviz, não o netlistsvg (decisão do Lace, API.md seção 10) |
| Full Build (F5) | Feito | F5: verifica e, se passou, simula |
| Cancelar (Shift+F5) | Feito | Parar (Shift+F5); o Core encerra a ferramenta com tudo o que ela iniciou |
| Icarus ou Verilator | Feito | menu Fluxo e preferência |
| Simulação de processador direto, sem testbench do projeto | Feito | alvo = processador; a AURORA só simulava pelo projeto |
| Prazo da simulação | Feito | preferência "Prazo da simulação" (o `--timeout` da CLI) |
| Botões habilitados conforme o `.spf` (topo, testbench) | Parcial | habilitados com projeto aberto; sem topo ou testbench, o erro do Core aparece traduzido, com a dica de onde resolver |
| Selo de command overrides | Depende do Lace | o Lace preserva `commandOverrides` no `.spf`, mas não os aplica |
| Testbench em Python (cocotb) | Depende do Lace | o bundle prevê o componente, o Core ainda não roda |
| GTKWave | Diferente | o bundle do Lace traz só o surfer-aurora |

## Consoles e terminal

| Terminal da AURORA | Estado | No Studio |
|---|---|---|
| TCMM | Feito | console C± |
| TASM | Feito | console ASM |
| TVERI | Feito | console Verilog |
| TWAVE | Feito | console Wave; mostra também as saídas de cada porta do processador |
| TPRISM | Feito | console PRISM |
| TCMD (shell real) | Feito | aba Terminal: o shell do usuário (PowerShell no Windows), com o `lace` no `PATH` |
| THTEST (teste de hardware) | Depende do Lace | não implementado no Lace (API.md, seção 10) |
| Links `arquivo:linha` clicáveis | Feito | |
| Modo verboso | Feito | preferência: mostra o comando de cada passo e as mensagens `info` |
| Painel de problemas | Feito | aba Problemas, com filtro; diagnósticos também viram marcadores no editor |
| Filtros por tipo de linha com contagem, nos consoles | Parcial | no painel Problemas; nos consoles, Fase 2 |
| Exportar log | Fase 2 | |

## Histórico

| Recurso da AURORA | Estado | No Studio |
|---|---|---|
| Histórico de compilação (Ctrl+Shift+H) | Feito | Relatórios do Lace: cada build, check, sim e synth grava um; lista, texto e comparação de síntese e de tempos |
| Histórico local de cada arquivo, com diff | Fase 3 | |
| Pontos de restauração e "rewind" | Fase 3 | junto com o assistente |

## Onda e esquemático

| Recurso da AURORA | Estado | No Studio, ou o que falta |
|---|---|---|
| Abrir a onda no Surfer em janela | Feito | Onda (Ctrl+F8), com a preferência em janela |
| Surfer embutido numa aba | Feito | o padrão; o cliente web lê o arquivo, sem `surfer server` (ADR 0011) |
| Escolher layout `.gtkw`, `.surf.ron`, `.sucl` | Fase 2 | o Core já recebe um layout (`ViewerOptions::layout`); falta a interface |
| Wave Configuration: escolher sinais por testbench | Depende do Lace | o Lace grava todos os sinais (`$dumpvars(0, tb)`) |
| Layouts gerados (grupos do processador, tradutores ASM e C±, números complexos) | Feito | `wave_layout` do Core (ADR 0011 do Lace), na aba e na janela |
| Esquemático: escolher módulo, zoom, arrastar, ajustar, abrir o SVG | Feito | aba Esquemático |
| Duplo clique num módulo abre o fonte | Fase 2 | |
| 82 skins do netlistsvg | Diferente | o desenho é o do Graphviz (decisão do Lace) |
| Simulação interativa com DigitalJS | Fase 3 | |
| Estatísticas da síntese | Feito | aba Síntese: células, fios, memórias e células por tipo (sem LUT nem temporização: não há mapeamento para FPGA) |

## Editor

| Recurso da AURORA | Estado | No Studio, ou o que falta |
|---|---|---|
| Monaco, abas provisórias, marcador de alteração, reabrir aba fechada | Feito | |
| Reordenar abas arrastando | Fase 2 | |
| Dividir o editor (até 3) | Feito | lado a lado, cada grupo com as suas abas; arrastar abas entre grupos, abrir ao lado, `Ctrl+\` e Ctrl+1..3 ([ADR 0009](adr/0009-editor-dividido-em-grupos.md)) |
| Realce de C± | Feito | gramática portada e conferida contra o léxico do YANC; corrige pontos em que a AURORA divergia (ver `src/editor/languages/cmm.ts`) |
| Realce do assembly do SAPHO | Feito | os 114 opcodes do YANC |
| Realce de Verilog e SystemVerilog | Feito | o do Monaco |
| Realce de MATLAB | Fase 2 | |
| Camada semântica tree-sitter | Fase 2 | |
| Sugestões: diretivas, palavras, funções de C±, trechos de Dirac | Feito | |
| LSP de Verilog (Verible, slang): lint, definição, referências, renomear, formatar | Fase 2 | as ferramentas não estão no bundle do Lace |
| Formatar com clang-format, Verible, black | Fase 2 | |
| Localizar, substituir, ir para a linha, minimapa, rolagem fixa | Feito | |
| Breadcrumbs, contorno (outline) | Fase 2 | |
| Abas de imagem, PDF e HTML | Fase 2 | |

## Aplicativo

| Recurso da AURORA | Estado | No Studio, ou o que falta |
|---|---|---|
| Paleta de comandos (Ctrl+Shift+P) | Feito | |
| Abrir arquivo do projeto pelo nome | Feito | Ctrl+P |
| Modo de símbolos da paleta (Ctrl+T) | Fase 2 | |
| Localizar nos arquivos | Feito | Busca (Ctrl+Shift+F), com expressão regular e maiúsculas |
| Substituir nos arquivos | Fase 2 | |
| Preferências: idioma, tema, editor, modo verboso | Feito | Preferências (Ctrl+,) |
| Atalhos configuráveis | Fase 2 | a tabela já está em `actions.ts` |
| Componentes: baixar, remover, conferir | Feito | Ferramentas do Lace: componentes, executáveis, compilador do Verilator, conferência dos hashes, instalar (`lace install`); remover componente o Lace não faz |
| Inglês e português | Feito | o YANC roda sempre em inglês (`-en`), por decisão do Lace |
| Temas | Feito | a AURORA tinha só o escuro; o Studio tem 15, com o Atlas de padrão e o Aurora Legacy nas cores da AURORA (ADR 0010) |
| Atualização | Parcial | procura e instala atualização do Lace e do bundle (`lace update`), na tela de ferramentas; atualização do próprio Studio, Fase 2 |
| Manual offline num navegador interno | Fase 2 | hoje, Ajuda > Manual do SAPHO abre o site |
| Sobre | Feito | |
| Assistente (Aurora Intelligence) | Fase 3 | ver abaixo |
| Painel de Git (GitHub e GitLab) | Fase 3 | |
| Gerenciador de bibliotecas Python (cocotb) | Depende do Lace | só faz sentido quando o Lace rodar cocotb |
| Tela de abertura, fundo animado, ícone personalizado | Diferente | o visual do Studio é neutro e sem enfeite (ADR 0005) |
| Extras do Windows (bateria, rede, Smart App Control, jumplist) | Diferente | fora do escopo |

## Também no Studio

Recursos que esta comparação não cobre, porque não estão no manual da AURORA
(`Aurora-Documentacao/source`) ou vêm de outro lugar:

| Recurso | De onde vem |
|---|---|
| Modo Vim no editor (`monaco-vim`), com `:w`, `:q`, `:wq` | pedido para o Studio |
| Limpar relatórios (todos, ou todos menos os N mais novos) | `lace report clean`, do Lace |
| Hierarquia de cada testbench, além da do design | a elaboração do Icarus |
| Mover arquivos na árvore mantendo o `.spf` | o Studio, compondo funções do Core |

## Fases

**Fase 1 (0.1, esta).** A IDE inteira em volta do Lace: projeto, explorador,
editor com C±, asm e Verilog, os fluxos (C±, Verilog, Wave, Rápida, PRISM)
com saída ao vivo e cancelamento, consoles por etapa, Problemas, terminal de
shell, esquemático, estatísticas, relatórios (comparar e limpar), hierarquia
elaborada pelo Icarus, ferramentas do bundle, preferências, português e
inglês, 15 temas (Atlas de padrão), modo Vim, editor dividido em até três
grupos, a onda numa aba com o layout dos processadores (variáveis, assembly,
linha do C±).

**Fase 2.** Fechar o que falta da AURORA sem depender de decisão grande:
escolher um layout próprio da onda, LSP de Verilog, formatação, substituir nos arquivos, exportar log, backup e
exemplos, atalhos configuráveis, manual offline, atualização do Studio.

**Fase 3.** O assistente (o papel da Aurora Intelligence: operar o Studio
pelas mesmas ações e comandos desta documentação), Git, histórico local e
pontos de restauração, simulação interativa do esquemático.

**Depende do Lace.** THTEST, escolha de sinais da onda,
command overrides, cocotb, renomear e apagar processador, renomear projeto.
Cada um é uma proposta para o `lace-core` antes de ser um botão aqui.
