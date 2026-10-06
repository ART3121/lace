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

## Fora do Lace (7)

Defeitos do YANC, do fork surfer-aurora e de um projeto de usuário (HITS). O Lace só embrulha essas ferramentas; a correção é nelas.

### O `cpppp` não marca a linha de origem (`#line`), e no fluxo C os erros e a linha da onda apontam para o `pp.cpp`

- **Área:** Externo (YANC)
- **Gravidade:** baixa (externo)
- **Chave:** `externo-cpppp-sem-line`

**Problema:** sobra de `layout-c-rotulado-como-cmm` (o rótulo `C` foi corrigido em 2026-10-04). O `cpppp` tira as diretivas e junta os `#include` sem escrever `#line`, então a linha 12 do `pp.cpp` não diz de que arquivo e linha veio. O Lace não tem como voltar ao fonte sem adivinhar (casar texto falha onde há macro).
**Esperado/onde:** o `cpppp` emitir `# <linha> "<arquivo>"` como o `cpp` do gcc (`CPPComp/Sources/cpppp.c`, `process_file`); o `cppcomp` contar as linhas por esses marcadores; o Lace já lê `arquivo:linha` dos diagnósticos.

### Os dois .spf do repositório hits não listam rtl/noise/noise_gauss.v e não simulam no Lace nem na AURORA

- **Área:** Externo (HITS)
- **Gravidade:** alta para o HITS; não é defeito do Lace
- **Chave:** `externo-hits-spf-desatualizado`

**Problema:** `projects/aurora_simulador/simulador.spf` e `projects/aurora/sim_pulsos.spf` (nipscernlab/hits, commit e81e68e) não listam `rtl/noise/noise_gauss.v`, o módulo do ruído gaussiano padrão desde 2026-10-02; o Icarus para com `Unknown module type: noise_gauss`. A CI do HITS não pega porque `verification/regress.py` compila por glob (`rtl/*/*.v`). Com a linha acrescentada, a simulação do Lace sai idêntica às goldens.
**Correção (no HITS, não no Lace):** acrescentar `..\\..\\rtl\\noise\\noise_gauss.v` em `synthesizableFiles` dos dois `.spf`; talvez a CI conferir os `.spf`.

### Com 3 ou mais portas de saída e memória de dados pequena, a porta 0 nunca é escrita

- **Área:** Externo (YANC/SAPHO)
- **Gravidade:** alta (externo)
- **Chave:** `externo-yanc-porta-0-sem-escrita`

**Problema:** `lace proc add f --outputs 3` com o fonte-modelo (`out(0, 0);`): `Output 0: no values` (com 2 saídas funciona). Na onda, `addr_out` fica `bx0`/`bx1` e `out_en` em `x`. Causa: `SAPHO/core.v:442` faz `addr_out <= addr[NBIOOU-1:0]` com `addr` de MDATAW bits (1, para MDATAS 2) e NBIOOU 2, então o bit alto fica X.
**Reproduzir:** `lace proc add f --outputs 3; lace sim -p f`
**Esperado/onde:** corrigir na biblioteca SAPHO do YANC (estender `addr` ou dimensionar MDATAW pelo NBIOOU).

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

**Problema:** `int v[2]; int w=5; int y=6; v[0]=1; v[2]=99; v[3]=98; out(0,w); out(0,y); out(0,v[0]);` dá `98 6 1`: `w` foi sobrescrito.
**Reproduzir:** o programa acima
**Esperado/onde:** o cmmcomp acusar índice constante fora do tamanho.

### Programa sem main() compila e gera JMP main para rótulo inexistente

- **Área:** Externo (YANC)
- **Gravidade:** média (externo)
- **Chave:** `externo-yanc-sem-main`

**Problema:** Só `int f(int a){ return a+1; }`: o build dá sucesso, o `.asm` tem `JMP main` sem `@main`, e o asmcomp aceita o rótulo indefinido. `funcoes.c:364` marca `mainok = 1` na primeira função, então o erro de `variaveis.c:74` nunca dispara.
**Reproduzir:** o programa acima; `lace build`
**Esperado/onde:** corrigir o `mainok` no cmmcomp e o asmcomp recusar rótulo indefinido.

### Mensagens de configuração erradas, NDSTAC 0 e NUIOIN enorme passam, divisão por zero sem aviso, erro de sintaxe na linha seguinte, crash com nome de 99+ caracteres

- **Área:** Externo (YANC)
- **Gravidade:** baixa (externo)
- **Chave:** `externo-yanc-mensagens-e-limites`

**Problema:** `--nbmant 30` dá 'the biggest integer this thing can hold is -2147483648!'; `--nbmant 0` aponta a linha do `#NUGAIN`; `#NDSTAC 0` só quebra no iverilog (`core.v:160`); `#NUIOIN 100000` gera testbench com portas aleatórias e 904 erros de sintaxe (estouro no asmcomp, dedução); `5/0` constante compila sem aviso e `2.5/0.0` dá 442360; `;` faltando aponta a linha seguinte (comum em yacc); nome de processador com 99 a 200 caracteres faz o cmmcomp morrer com sinal 11.
**Reproduzir:** ver o relatório do agente SAPHO
**Esperado/onde:** melhorias no YANC. Do lado do Lace, o `proc add` já recusa esses parâmetros e nomes (corrigido em 2026-10-04); um `.spf` editado à mão ainda chega ao YANC com eles.
