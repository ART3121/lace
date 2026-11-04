# Pendências do teste de fogo

O teste de fogo de 2026-10-04 exercitou o Core, a CLI e o Lace Studio com
casos extremos: projeto e `.spf` (formatos quebrados, nomes, concorrência),
Verilog (diagnósticos, simulação longa, volume, síntese, hierarquia) e
processadores SAPHO (YANC, entradas, layout da onda). Cada item abaixo foi
reproduzido com os comandos indicados, salvo os marcados como suspeita.

Todos os casos do Lace e do Lace Studio foram corrigidos na mesma data
(CHANGELOG: "Correções do teste de fogo", "Correções médias do teste de
fogo" e "Correções baixas do teste de fogo"); as duas decisões pendentes
foram tomadas: os caminhos de fora da raiz e o resgate pela cauda. Ficaram só os defeitos das ferramentas que o Lace
embrulha e de um projeto de usuário, que se corrigem nos repositórios
deles.

Ao corrigir um item, tire-o daqui e registre no CHANGELOG.

## Fora do Lace (6)

Defeitos do YANC, do fork surfer-aurora e de um projeto de usuário (HITS). O Lace só embrulha essas ferramentas; a correção é nelas.

### Suspeita: no macOS, os compiladores do YANC (desde a 7.1) não acham `Macros/`, `SAPHO/` e `Header/`

- **Área:** Externo (YANC)
- **Gravidade:** alta (externo): sem isso, nenhum processador compila no macOS com a 7.1
- **Chave:** `externo-yanc-exe-dir-macos`

**Problema:** deduzido do código, não reproduzido num Mac. Na 7.1, cada ferramenta acha as pastas da biblioteca ao lado do próprio executável (`Compilers/common/exe_dir.c`, `exe_dir`), e as opções que as davam (`-m`, `-d`) saíram. Fora do Windows, `exe_dir` lê `/proc/self/exe`, que não existe no macOS; o caminho volta vazio, `exe_find_dir` devolve `NULL`, e o `cmmcomp` e o `asmcomp` param com o erro de pasta que falta ("instale o YANC de novo"). A CI do YANC não roda no macOS. Conferido no fonte da v9.1 (2026-10-22): o `exe_dir` continua só com o `/proc/self/exe`.
**Reproduzir:** num Mac, `cmmcomp prog.cmm` com o YANC 7.1 instalado, ou `aurora build` com o bundle `darwin-arm64` de 2026-10-13 ou de 2026-10-22.
**Esperado/onde:** um ramo `__APPLE__` em `exe_dir` com `_NSGetExecutablePath` (de `<mach-o/dyld.h>`) seguido de `realpath`, para resolver os symlinks como o `/proc/self/exe` resolve no Linux.

### Os dois .spf do repositório hits não listam rtl/noise/noise_gauss.v e não simulam no Lace nem na AURORA

- **Área:** Externo (HITS)
- **Gravidade:** alta para o HITS; não é defeito do Lace
- **Chave:** `externo-hits-spf-desatualizado`

**Problema:** `projects/aurora_simulador/simulador.spf` e `projects/aurora/sim_pulsos.spf` (nipscernlab/hits, commit e81e68e) não listam `rtl/noise/noise_gauss.v`, o módulo do ruído gaussiano padrão desde 2026-10-02; o Icarus para com `Unknown module type: noise_gauss`. A CI do HITS não pega porque `verification/regress.py` compila por glob (`rtl/*/*.v`). Com a linha acrescentada, a simulação do Lace sai idêntica às goldens.
**Correção (no HITS, não no Lace):** acrescentar `..\\..\\rtl\\noise\\noise_gauss.v` em `synthesizableFiles` dos dois `.spf`; talvez a CI conferir os `.spf`.

### `load_state` zera o arquivo escolhido de um surfer server e o cliente recarrega a onda, perdendo o layout

- **Área:** Externo (surfer-aurora)
- **Gravidade:** média (externo)
- **Chave:** `externo-surfer-aurora-load-state`

**Problema:** no fork v0.7.0-nips.10, `load_state` troca o `UserState` inteiro; `selected_server_file_index` é `#[serde(skip)]` e volta a `None`, e a próxima consulta de status recarrega a onda com `Clear`. Por isso a aba de onda do Studio lê o arquivo direto, sem `surfer server`, com limite de 256 MB.
**Correção (no fork):** preservar `selected_server_file_index`, `surver_file_infos`, `surver_url` no `load_state` (`libsurfer/src/state.rs`). Também: o cliente web só processa mensagens ao desenhar (o Studio injeta `InvalidateDrawCommands` por 15 s).

### Escrita fora do array com índice constante compila sem aviso e corrompe outra variável

- **Área:** Externo (YANC)
- **Gravidade:** média (externo)
- **Chave:** `externo-yanc-array-fora-do-limite`

**Problema:** `int v[2]; int w=5; int y=6; v[0]=1; v[2]=99; v[3]=98; out(0,w); out(0,y); out(0,v[0]);` dá `98 6 1`: `w` foi sobrescrito. Continua na v9.1 (2026-10-22).
**Reproduzir:** o programa acima
**Esperado/onde:** o cmmcomp acusar índice constante fora do tamanho.

### Programa sem main() compila e gera JMP main para rótulo inexistente

- **Área:** Externo (YANC)
- **Gravidade:** média (externo)
- **Chave:** `externo-yanc-sem-main`

**Problema:** Só `int f(int a){ return a+1; }`: o build dá sucesso, o `.asm` tem `JMP main` sem `@main`, e o asmcomp aceita o rótulo indefinido. `funcoes.c:364` marca `mainok = 1` na primeira função, então o erro de `variaveis.c:74` nunca dispara. Continua na v9.1 (2026-10-22): o `aurora build` dá sucesso.
**Reproduzir:** o programa acima; `aurora build`
**Esperado/onde:** corrigir o `mainok` no cmmcomp e o asmcomp recusar rótulo indefinido.

### Mensagens de configuração erradas, NDSTAC 0 e NUIOIN enorme passam, divisão por zero sem aviso, erro de sintaxe na linha seguinte, crash com nome de 99+ caracteres

- **Área:** Externo (YANC)
- **Gravidade:** baixa (externo)
- **Chave:** `externo-yanc-mensagens-e-limites`

**Problema:** `--nbmant 30` dá 'the biggest integer this thing can hold is -2147483648!'; `--nbmant 0` aponta a linha do `#NUGAIN`; `#NDSTAC 0` só quebra no iverilog (`core.v:160`); `#NUIOIN 100000` gera testbench com portas aleatórias e 904 erros de sintaxe (estouro no asmcomp, dedução); `5/0` constante compila sem aviso e `2.5/0.0` dá 442360; `;` faltando aponta a linha seguinte (comum em yacc); nome de processador com 99 a 200 caracteres faz o cmmcomp morrer com sinal 11.
**Reproduzir:** ver o relatório do agente SAPHO
**Esperado/onde:** melhorias no YANC. Do lado do Lace, o `proc add` já recusa esses parâmetros e nomes (corrigido em 2026-10-04); o build recusa antes do YANC um nome inválido vindo de um `.spf` editado à mão (`validate_source`), mas um fonte editado à mão ainda chega ao YANC com esses parâmetros.
