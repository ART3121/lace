# 0012. Caminho relativo no mesmo repositório e resgate pela cauda

- **Status:** Aceita
- **Data:** 2026-10-04

## Contexto

A ADR 0003 adotou o `.spf` da AURORA com as regras dela para caminhos
(`js/project/caminho_de_projeto.ts`): dentro da pasta do projeto, relativo à
raiz; fora dela, absoluto. O teste de fogo de 2026-10-04 achou dois
problemas nessa regra.

- **Arquivo de fora da raiz.** Projetos como o HITS (nipscernlab/hits) têm o
  `.spf` em `projects/aurora_simulador/` e o Verilog em `rtl/`, no mesmo
  repositório, e gravam `..\..\rtl\...` à mão para funcionar em qualquer
  clone. O `lace add` (e o Adicionar e o arrastar do Studio) gravava o
  caminho absoluto da máquina de quem adicionou, e o `.spf` deixava de
  funcionar no clone de outra pessoa. O arrastar do Studio copiava o arquivo
  para a raiz, o que divergia do original.
- **Absoluto de outra máquina.** Um `.spf` levado de um Windows
  (`C:\Users\aluno\p\rtl\alu.v`) abria no Lace com o arquivo inexistente. A
  AURORA resgata pela cauda (`resgatarPelaCauda`): tenta, dentro da pasta do
  projeto de hoje, `aluno/p/rtl/alu.v`, `p/rtl/alu.v`, `rtl/alu.v` e
  `alu.v`, nessa ordem, e usa o primeiro que existir.

A decisão foi do usuário (2026-10-04), entre gravar relativo só no mesmo
repositório git, sempre relativo, ou manter o absoluto.

## Decisão

- **Gravar** (`files.rs`, `store`): dentro da raiz, relativo à raiz, como
  antes. Fora da raiz, relativo com `..` quando a raiz e o arquivo estão no
  mesmo repositório git (a primeira pasta acima da raiz com `.git`, pasta ou
  arquivo); absoluto quando não. Sempre com `/`, que a AURORA também lê.
- **Ler** (`files.rs`, `resolve`): um caminho absoluto que não existe nesta
  máquina é procurado pela cauda dentro da raiz, como na AURORA, da cauda
  mais longa para a mais curta. O caminho inteiro nunca é tentado, e uma
  cauda com `..` não conta.
- **Consertar.** Abrir continua só lendo (ADR 0003). O caminho achado pela
  cauda vira aviso de `Project::issues` (`rescued_path`), e a próxima
  gravação do `.spf`, por qualquer mudança, troca o absoluto antigo pelo
  caminho de hoje, na forma acima.
- **Arrastar no Studio** registra o arquivo no lugar, sem copiar, como o
  menu Adicionar.

## Consequências

- Um projeto dentro de um repositório git é portátil mesmo com fontes fora
  da pasta do `.spf`, e o `.spf` do HITS continua com os `..` dele depois de
  um `lace add`.
- Fora de repositório, nada muda: um arquivo de outro disco ou de outra
  pasta pessoal vai absoluto, como na AURORA.
- A AURORA lê o relativo com `..` (resolve contra a raiz) e o absoluto
  consertado; os dois programas continuam abrindo o mesmo projeto.
- O resgate pode achar um homônimo quando só o nome do arquivo casa (a
  última tentativa). É o mesmo risco da AURORA, e o aviso diz qual arquivo
  foi usado.
- Cada leitura de um absoluto inexistente faz alguns `stat` a mais; um `.spf`
  normal não tem nenhum.
