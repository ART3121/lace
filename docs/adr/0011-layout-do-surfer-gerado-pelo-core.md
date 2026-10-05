# 0011. O Core gera o layout do Surfer para os processadores SAPHO

- **Status:** Aceita
- **Data:** 2026-10-04

## Contexto

Depois de simular um processador, a AURORA abria a onda com as variáveis
do programa, a instrução de assembly e a linha do C± de cada ciclo, em
grupos. Quase tudo isso já vem do YANC: o `<proc>.v` tem, no bloco
`YANC_SIM_VIS`, uma cópia de cada variável (`me1_f_<função>_v_<nome>_e_`,
`me2_`, `comp_me3_`, `arr_me<T>_..._e_<NNNN>`), o PC atrasado de dois
ciclos (`valr2`) e a linha do fonte (`linetabs`), e o testbench grava todos.
O que faltava era o lado do visualizador: a AURORA lia o cabeçalho da onda,
achava os processadores, convertia `trad_opcode.txt` e `trad_cmm.txt` em
tradutores de valor do Surfer (*mapping translators*) e escrevia um
`.surf.ron` com os grupos (`js/wave/surfer_layout_writer.ts`,
`js/compilation/layout_do_surfer.ts`). O Lace abria a onda crua.

Duas coisas tinham de ser decididas: onde fica essa regra e onde ficam os
tradutores. O surfer-aurora lê tradutores da pasta de configuração do
usuário e de `.surfer/mappings/` relativo à pasta onde roda
(`find_user_mapping_translators`, `libsurfer/src/translation/mod.rs`); a
AURORA escrevia na pasta do usuário.

## Decisão

A regra fica no Core, em `crates/lace-core/src/wave_layout.rs`, como pede a
ADR 0001:

- `wave_layout(onda)` lê o cabeçalho do VCD, acha cada processador (o
  escopo com `valr2` e `linetabs`, com o nome tirado do subescopo
  `p_<nome>.core`), lê as tabelas da pasta temporária dele no projeto e
  devolve o `WaveLayout` em memória: o `.surf.ron`, os tradutores e um
  resumo por processador (`WaveProcessor`). Onda sem processador, ou que não
  é VCD, não tem layout.
- `prepare_wave_layout(onda)` grava isso em `.lace/Temp/surfer/<onda>/`: o
  estado e os tradutores em `.surfer/mappings/`. `ViewerOptions::with_layout`
  abre o Surfer com `-s` e com a pasta do layout como pasta de trabalho
  (`ViewerOptions::working_dir`), onde ele acha os tradutores. Nada vai para
  a configuração do usuário.
- `lace wave` e `lace sim --open` aplicam o layout sozinhos; `--no-layout`
  desliga.
- O cliente web do surfer-aurora (o mesmo Surfer em WebAssembly) entra no
  bundle, em `surfer-aurora/web/`, da mesma tag do executável
  (`scripts/bundle.py`, `bundle/versions.json`); `Toolchain::surfer_web_dir`
  o acha. Uma interface que mostra a onda numa página usa o `WaveLayout` em
  memória e os comandos do fork `load_mapping_translator_from_url` e
  `load_state_from_url`.

## Consequências

- O `.surf.ron` segue o formato do surfer-aurora do bundle (base Surfer
  0.7.0, com os três campos de `WaveData` sem padrão). Um surfer-aurora novo
  pode mudar o formato; o teste com o bundle (`icarus_simulates_both_languages`)
  confere o layout, mas não o carregamento no Surfer, que exige display.
- O Surfer reacha cada sinal pelo caminho e pelo nome, então os
  identificadores do estado são marcadores e o layout sobrevive a uma
  simulação nova.
- Complexos são traduzidos pelos valores que aparecem na onda: o Core lê o
  corpo do VCD só quando há sinal complexo.
- Fora dos processadores, o layout mostra só os sinais da raiz do testbench;
  o resto do projeto continua na hierarquia do Surfer. A AURORA listava
  todos.
- O `load_state` do surfer-aurora zera o arquivo escolhido de um
  `surfer server` (`selected_server_file_index`, que não vai no estado), e o
  cliente então recarrega a onda e perde o layout. Enquanto o fork não
  preservar esse campo, um cliente web deve ler a onda direto do arquivo, não
  por `surfer server`.
