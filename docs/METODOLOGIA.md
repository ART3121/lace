# Metodologia: como o Lace cresce

Este documento diz como o código do Lace se organiza e como a API dele muda
sem quebrar quem a usa. Ele vale para quem vai acrescentar uma função nova,
mudar uma existente ou revisar a mudança de outra pessoa.

Ele não repete o que já está escrito em outro lugar. Preparar a máquina,
rodar os testes e as receitas das mudanças comuns estão em
[CONTRIBUTING.md](../CONTRIBUTING.md). A referência de cada função do Core
está em [API.md](API.md). A lista de comandos e de métodos do `lace serve`
está em [CLI.md](CLI.md). O que é só do Studio está em
[studio/docs/DEVELOPMENT.md](../studio/docs/DEVELOPMENT.md).

As seções 1 a 7 valem desde já. A seção 8 é a proposta do que muda no
código até a 1.0, para que um método novo custe menos arquivos do que custa
hoje.

## 1. A regra das camadas

O repositório tem uma biblioteca com toda a regra de negócio e várias
interfaces em volta dela. Cada camada só conhece a de baixo.

```
lace-core          a regra: projeto, fluxos, ferramentas, erros e o contrato (protocol)
  lace-learn       os exercícios do lace learn, sobre o core
  lace-installer   instalar, atualizar e desinstalar, sobre o core
  lace-agent       o assistente de IA, sobre o core (experimental)
lace-cli           o binário lace: os comandos de texto, o lace serve, o lace mcp e o lace chat
studio/src-tauri   o backend do Lace Studio: comandos Tauri sobre os crates acima
studio/src         a interface do Studio, em TypeScript
editors/vscode     a extensão do VS Code, em TypeScript, pelo lace serve ou pela CLI
```

Cinco regras seguem dessa figura.

1. **A regra mora no core.** Toda decisão é uma função do `lace-core`, com
   teste lá: o que rodar, em que ordem, o que validar, que arquivo escolher.
   Se um `if` de uma interface muda o que o Lace faz, ele está no lugar
   errado.
2. **As interfaces só traduzem.** A CLI, o `lace serve`, o `lace mcp`, os
   comandos do Studio e a extensão abrem o projeto, chamam o core e mostram
   o resultado. Elas escolhem como mostrar, nunca o que fazer.
3. **Uma camada não conhece a de cima.** O core não sabe que existe CLI nem
   Studio. As mensagens dele não citam comandos: a dica com o comando que
   resolve é posta pela CLI, no campo `hint`.
4. **Uma capacidade nova nasce como método do protocolo.** Mesmo quando o
   primeiro cliente é o Studio, ela entra em `lace_core::protocol` e no
   `lace serve`. Assim chega à extensão, ao `lace mcp` e a scripts sem ser
   escrita de novo.
5. **O TypeScript não copia tipo do Rust à mão.** Os tipos vêm de
   `docs/schema/`, gerados dos tipos do core. O `lace-types.ts` do Studio e
   o da extensão são gerados; o que se escreve à mão é só o que existe
   apenas numa interface.

## 2. O que é uma API do Lace

Uma API do Lace é um **método do protocolo**. Ele tem seis peças, sempre as
mesmas. O exemplo é o `history.compare`, que compara dois relatórios.

| Peça | Onde mora | No exemplo |
|---|---|---|
| Nome | `METHODS` e o `#[serde(rename)]` da variante, em `crates/lace-core/src/protocol.rs` | `history.compare` |
| Pedido | uma variante de `Request`, no mesmo arquivo | `Request::HistoryCompare { project, id, against }` |
| Resposta | uma variante de `Response`, com um tipo do core | `Response::Comparison(Box<RunComparison>)` |
| Função | o módulo do domínio, no core | `history::compare_reports` |
| Erros | as variantes de `LaceError` que a função devolve, cada uma com `code()` | |
| Documentação | o `///` da variante e de cada campo | vira a descrição no schema e no `lace mcp` |

Um comando da CLI não é uma API. Ele é uma interface para pessoas, com texto
e `--json`, e pode juntar vários métodos ou mostrar um só de outro jeito. O
contrato do `--json` da CLI continua valendo, como diz o CONTRIBUTING.md,
mas quem escreve um programa que fala com o Lace usa o protocolo.

## 3. Nomes

| O quê | Regra | Exemplo |
|---|---|---|
| Método | `domínio.verbo` ou `domínio.verbo_objeto`, em minúsculas e em inglês | `project.status`, `history.plan_cleanup` |
| Variante de `Request` | o método em CamelCase | `HistoryPlanCleanup` |
| Função do core | `verbo_objeto`, no módulo do domínio | `history::compare_reports` |
| Ferramenta do `lace mcp` | o método com `_` no lugar do ponto | `history_compare` |
| Comando do Studio | igual à ferramenta do `lace mcp` | `history_compare` |
| Função no frontend do Studio | `api.<domínio>.<verbo>`, em `studio/src/ipc/api.ts` | `api.history.compare` |
| Código de erro | `snake_case`, em inglês, e nunca muda depois de publicado | `experimental_disabled` |

Os domínios de hoje são `project`, `processor`, `flow`, `wave`, `history`,
`toolchain`, `experimental`, `fpga`, `update`, `install` e `learn`. Um
domínio novo ganha um módulo próprio no core e um arquivo próprio em
`studio/src-tauri/src/commands/`. Há exceções antigas à tabela, como
`fpga.config`, cuja variante é `FpgaConfigGet`. Elas ficam, mas não servem
de modelo.

## 4. O ciclo de vida de um método

Um método está sempre num de três estados.

- **Experimental.** Pode mudar ou sair em qualquer versão menor, sem o
  período de obsoleto. Antes da 1.0, todos os métodos estão aqui. Depois
  dela, todo método novo começa aqui até uma versão o declarar estável.
  Uma área inteira que ainda vai mudar muito, como a placa FPGA e o
  assistente de IA, fica também atrás de um recurso de
  `lace_core::experimental`: desligado, o Studio esconde a área, e a CLI,
  os comandos do Studio e os fluxos do core (`flows::run`) recusam com
  `experimental_disabled`, pelo `experimental::require`.
- **Estável.** Só muda de forma compatível, nos termos da seção 5.
- **Obsoleto.** Continua respondendo por pelo menos uma versão menor
  inteira, com o substituto escrito na documentação e no CHANGELOG. Só sai
  numa versão maior, que também sobe o `PROTOCOL_VERSION`.

Antes da 1.0 tudo é experimental, então ainda não há o que marcar no código.
A marca entra com o registro dos métodos (seção 8, fase 3). Lançar a 1.0 é
o ato de marcar quais métodos ficam estáveis.

## 5. Compatibilidade e versões

Há três números de versão, e cada um diz uma coisa diferente.

| Número | Onde | Quando sobe |
|---|---|---|
| Versão do Lace | os seis lugares de [RELEASE.md](RELEASE.md), seção 6.1 | a cada release, pelas regras da seção 2 de [RELEASE.md](RELEASE.md) |
| `PROTOCOL_VERSION` | `crates/lace-core/src/protocol.rs`, devolvido pelo `initialize` | só quando um cliente antigo deixaria de funcionar |
| Versão da extensão | `editors/vscode/package.json` | a cada `.vsix` publicado com mudança, para o editor reconhecer a atualização |

O que conta como compatível:

| Mudança | Compatível |
|---|---|
| Um método novo | sim |
| Um campo opcional novo no pedido, com padrão que mantém o comportamento de antes | sim |
| Um campo novo na resposta | sim |
| Uma variante nova num enum da resposta marcado `#[non_exhaustive]` | sim, e os clientes ignoram o valor que não conhecem |
| Um código de erro novo | sim |
| Um campo obrigatório novo no pedido | não |
| Tirar ou renomear um método, um campo ou uma variante | não |
| Mudar o tipo de um campo, o significado dele ou um valor padrão | não |
| Mudar um código de erro já publicado | nunca |

As mudanças compatíveis da tabela acrescentam algo ao contrato, e por isso
são novidade: entram numa versão menor, nunca numa versão de correção, com
uma linha no CHANGELOG ([RELEASE.md](RELEASE.md), seção 2). Uma
incompatível, antes da 1.0, também entra numa versão menor, com a linha no
CHANGELOG dizendo o que quebra e o `PROTOCOL_VERSION` somado de um. Depois
da 1.0, só numa versão maior, depois do período de obsoleto.

**O portão é o diff de `docs/schema/`.** Os arquivos de lá saem dos tipos
por geração e nunca são editados à mão. Toda revisão de uma mudança de
contrato lê esse diff e confere, com a tabela acima, se ela é compatível.

Um cliente descobre o que o Lace sabe fazer pela lista de métodos que o
`initialize` devolve, não comparando versões. A extensão do VS Code ainda
compara versões quando usa a CLI direto, pela tabela `FEATURES` de
`editors/vscode/src/lace/version.ts`. Uma função nova que depende da CLI
ganha uma linha lá.

## 6. Passo a passo de um método novo

Este é o caminho como ele está na 0.9.0. Cada passo diz o arquivo. A seção
8 propõe como encurtá-lo.

1. **Domínio e nome.** Escolha pela seção 3. Antes, confira se um campo
   opcional novo num método existente não resolve: um método a menos é um
   método a menos para manter.
2. **A função no core.** No módulo do domínio. Ela recebe o `&Toolchain`
   se roda ferramenta, o `&Project` ou o `&Processor`, as opções numa
   struct `XOptions` com `Default`, `Deserialize` e `JsonSchema`, e o
   `&Control` por último se roda ferramenta. Ela devolve
   `Result<T, LaceError>`, com `T` em `Serialize` e `JsonSchema` e cada
   campo documentado. O teste fica no próprio módulo ou em
   `crates/lace-core/tests/`. As regras do código do core estão no
   CONTRIBUTING.md, em "Regras do código".
3. **O protocolo.** Em `crates/lace-core/src/protocol.rs`: a variante de
   `Request`, com o `#[serde(rename = "domínio.verbo")]` e o `///` em cada
   campo; a variante de `Response`, se o tipo da resposta é novo; o nome em
   `METHODS`. Se o método só lê, ele entra em `is_read_only`; se mexe na
   instalação, em `is_installation`. O teste
   `the_method_list_matches_the_request_enum` confere `METHODS` contra o
   enum.
4. **O despacho.** Um braço em `Server::handle_with`, em
   `crates/lace-cli/src/serve.rs`: abrir o projeto, chamar a função do core
   e embrulhar o resultado na `Response`. Nenhuma regra entra aqui. Atenção:
   o `match` termina num braço `other =>` que responde `unsupported`, então
   o compilador não avisa quando falta o braço. O teste de ponta a ponta vai
   em `crates/lace-cli/tests/serve.rs`.
5. **O `lace mcp`.** Se o método serve a um agente de IA, ele ganha uma
   linha em `TOOLS`, em `crates/lace-cli/src/mcp/mod.rs`. Fica de fora o que
   abre janela, o que apaga sem um plano antes e o que só faz sentido numa
   interface com pessoa. O resumo em texto, em `mcp/summary.rs`, é
   opcional: sem ele, o agente recebe o JSON formatado.
6. **Schemas e tipos gerados.** Gere de novo e leia o diff:

   ```bash
   LACE_UPDATE_SCHEMA=1 cargo test -p lace-cli --bin lace schema
   ```

   ```bash
   cd studio && npm run gen:types && cd ../editors/vscode && npm run gen:types
   ```

7. **As interfaces que vão usar o método.** Só as que precisam dele agora.
   - CLI: a receita "Acrescentar um comando ou uma opção à CLI" do
     CONTRIBUTING.md.
   - Studio: a receita "Um comando novo no backend" do DEVELOPMENT.md. O
     comando chama a mesma função do core que o braço do despacho chama, e
     nunca a reescreve.
   - Extensão do VS Code: o método na interface `LaceClient`
     (`editors/vscode/src/lace/client.ts`), em `serveClient.ts` e, se houver
     comando de CLI equivalente, em `cliClient.ts`.
8. **Documentação.** A função em [API.md](API.md); o método na lista do
   `lace serve` em [CLI.md](CLI.md); o comando do Studio em
   [IPC.md](../studio/docs/IPC.md), se houver; uma linha no `CHANGELOG.md`,
   e no `studio/CHANGELOG.md` se o Studio mudou.

### Pronto quando

- [ ] A regra está no core, com teste que falha sem a mudança.
- [ ] O método está em `METHODS`, no `Request`, no despacho e num teste de
      `crates/lace-cli/tests/serve.rs`.
- [ ] A decisão sobre o `lace mcp` foi tomada: na lista, ou de fora por um
      dos motivos do passo 5.
- [ ] `docs/schema/` e os dois `lace-types.ts` foram gerados de novo, e o
      diff é compatível pela seção 5, ou a incompatibilidade está no
      CHANGELOG.
- [ ] Nenhuma interface decide o que o core já decide.
- [ ] A documentação e o CHANGELOG acompanham.
- [ ] As verificações de "Antes de mandar a mudança", do CONTRIBUTING.md,
      passaram.

### Mudar um método existente

Primeiro classifique a mudança pela tabela da seção 5. Se ela é compatível,
o caminho é o mesmo dos passos 2 a 8, só nas peças que mudam. Se não é,
prefira um método novo ao lado do antigo e marque o antigo como obsoleto.
Mudar a forma de um método em uso quebra todo cliente que já existe, e o
custo disso cai em quem não participou da decisão.

### Tornar obsoleto e tirar

1. Escreva no `///` da variante que o método está obsoleto e qual o
   substituto. A descrição vai para o schema e para o `lace mcp`.
2. Uma linha no CHANGELOG da versão dizendo que o método ficou obsoleto e
   qual o substituto.
3. O método continua respondendo por pelo menos uma versão menor inteira.
4. Na versão maior seguinte, ele sai de todas as peças da seção 2, e o
   `PROTOCOL_VERSION` sobe.

## 7. Fluxo de trabalho e revisão

- **Branch.** Uma por assunto, a partir da `main`: `feat/<assunto>`,
  `fix/<assunto>` ou `docs/<assunto>`. A mudança chega na `main` por pull
  request.
- **Commit.** Um assunto por commit, com a mensagem em português e o
  prefixo da área: `Core:`, `CLI:`, `Studio:`, `VS Code:`, `Instalador:`,
  `Learn:`, `Agente:`, `Tipos gerados:` ou `Docs:`. A mudança de contrato e
  a geração dos tipos podem ir em commits separados, para o diff de
  `docs/schema/` ser lido sozinho.
- **Descrição do pull request.** O que muda, por quê, como foi testado e se
  muda o contrato: nenhuma mudança, compatível ou incompatível.
- **Quem revisa o contrato.** Uma mudança em `protocol.rs`, nos códigos de
  `error.rs` ou em `docs/schema/` passa pela revisão de quem cuida do
  contrato. Até o `CODEOWNERS` da seção 8 existir, isso é combinado no pull
  request.
- **Quem submete responde pelo código.** Vale também para código escrito
  por um assistente de IA, como o CONTRIBUTING.md já diz.

## 8. O que muda no código até a 1.0

Esta seção é uma proposta. Nada dela está feito ainda.

### Onde está o custo hoje

- **Um método mexe em muitos arquivos.** O `history.compare` aparece em 14
  arquivos, fora os tipos gerados. Um método usado por todos os clientes
  passa por uns 15: core, protocolo, despacho, `lace mcp`, CLI, Studio,
  extensão e documentação.
- **O despacho existe duas vezes.** O braço do `lace serve` e o comando do
  Studio fazem a mesma coisa: abrir o projeto, chamar o core, devolver o
  resultado. O Studio tem 86 comandos Tauri, e boa parte só repete o
  despacho.
- **Algumas respostas não têm tipo.** `update.check`, `update.apply`,
  `install.add` e `learn.check` respondem `Response::Other`, um JSON solto,
  porque o core não pode depender do `lace-installer` nem do `lace-learn`.
- **Faltar um braço do despacho não falha em nenhum teste.** O braço final
  `other =>` responde `unsupported` em tempo de execução.
- **Os testes Rust não rodam no GitHub.** O `release.yml` só monta os
  instaladores, e o `vscode.yml` só cobre a extensão. O `ci.yml`, que os
  rodava nos três sistemas, saiu em 2026-10-06.

### As fases

Cada fase vale sozinha e pode parar ali.

1. **A rede de segurança.** Um `cargo fmt --all` do repositório inteiro,
   num commit só e sem outra mudança, porque hoje dez arquivos estão
   fora do `rustfmt`. Depois, CI a cada push e pull request, nos três
   sistemas: `cargo fmt --check`, `clippy`, os testes sem bundle, o
   `cargo doc`, a conferência de `docs/schema/` e o typecheck dos dois
   projetos TypeScript. Um teste que manda cada método de `METHODS` ao
   `lace serve` e falha se algum responder `unsupported`. Um `CODEOWNERS`
   para `protocol.rs`, `error.rs` e `docs/schema/`, e um modelo de pull
   request com as perguntas da seção 7.
2. **Um despacho só.** Um crate novo, `lace-service`, acima do core, do
   `lace-installer` e do `lace-learn`, com uma função
   `handle(contexto, Request, cancelamento, notificações) -> Result<Response>`.
   O `lace serve` e o `lace mcp` viram só transporte. No Studio, os comandos
   que só repetem o despacho dão lugar a um comando genérico, `lace_call`.
   Ficam próprios os comandos que são do Studio: as operações longas com
   canal, os arquivos, o terminal, o vigia de arquivos, as preferências e a
   janela. As respostas do update, do install e do learn ganham tipo.
3. **O método declarado num lugar só.** Uma macro, como a `reports!` da
   CLI, em que cada método declara uma vez o nome, o estado da seção 4, se
   só lê, se é da instalação, se vai para o `lace mcp` e o resumo. `METHODS`,
   `is_read_only`, `is_installation` e a lista do `lace mcp` passam a sair
   dela. Um teste confere que cada método declarado tem braço no despacho,
   e o braço `other =>` deixa de existir.
4. **O contrato testado e a compatibilidade conferida pela máquina.** Casos
   de conformidade, pares de pedido e resposta em JSON, rodados contra o
   `lace serve` e contra os clientes em TypeScript. Na CI, uma comparação de
   `docs/schema/` com a última release que classifica cada mudança pela
   tabela da seção 5 e falha numa incompatível sem o `PROTOCOL_VERSION`
   somado.

Depois da fase 3, um método novo custa isto:

| Peça | Hoje | Depois da fase 3 |
|---|---|---|
| Função e teste no core | sim | sim |
| Variante em `Request` e `Response` | sim | sim |
| `METHODS`, `is_read_only`, `is_installation` | à mão | saem do registro |
| Braço no despacho | no `lace serve`, sem aviso se faltar | no `lace-service`, com teste que exige |
| Linha no `lace mcp` | à mão | sai do registro |
| Comando no Studio | um por método | nenhum, pelo `lace_call` |
| Tipos em TypeScript | gerados | gerados |
| Documentação e CHANGELOG | sim | sim |

A ordem sugerida é a das fases. A fase 1 é pequena e protege as outras. A
fase 2 é a que mais simplifica o Studio. A fase 3 depende da 2, e a 4
depende da 1.
