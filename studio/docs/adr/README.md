# Decisões de arquitetura

Cada decisão que molda o Studio, com o contexto e as consequências. Uma
decisão nova é um arquivo novo, numerado; uma decisão que muda ganha outro
arquivo, e o antigo passa a "Substituída por" (ou "emendada pela", quando
só uma parte muda).

| ADR | Decisão |
|---|---|
| [0001](0001-tauri-e-react-sobre-o-lace-core.md) | Tauri 2 e React, com o `lace-core` como biblioteca no backend |
| [0002](0002-fluxos-compostos-isolados-no-backend.md) | A composição dos fluxos fica isolada em `flows.rs` até ir para o Core |
| [0003](0003-um-editor-monaco-um-modelo-por-arquivo.md) | Um editor Monaco, um modelo por arquivo |
| [0004](0004-consoles-por-etapa-com-os-nomes-da-aurora.md) | Consoles xterm.js por etapa, com os nomes da AURORA |
| [0005](0005-visual-neutro.md) | Visual neutro, sem a paleta da AURORA |
| [0006](0006-instalar-e-atualizar-pela-cli.md) | Instalar e atualizar componentes pela CLI `lace` |
| [0007](0007-azul-do-cern-nos-detalhes.md) | O azul do CERN nos detalhes (emenda a 0005) |
| [0008](0008-a-marca-do-lace.md) | A marca do Lace no lugar do logo provisório (emenda a 0005) |
| [0009](0009-editor-dividido-em-grupos.md) | O editor dividido em até três grupos, cada um com o seu Monaco (emenda a 0003) |
| [0010](0010-temas.md) | Temas, com o Atlas de padrão e o Aurora Legacy com a paleta da AURORA (emenda a 0005 e 0007) |
| [0011](0011-onda-numa-aba.md) | A onda numa aba, com o cliente web do Surfer lendo o arquivo por um servidor local; janela como opção |

As decisões do Lace, que o Studio segue, estão em [`../../../docs/adr/`](../../../docs/adr/). As que
mais pesam aqui: 0001 (toda regra no Core, interfaces são cascas), 0002 (só
ferramentas do bundle), 0003 (formato `.spf` da AURORA), 0007 (cancelamento
e saída ao vivo), 0008 (o JSON é gerado dos tipos) e 0013 (o Studio neste
repositório e no bundle, como componente do instalador).
