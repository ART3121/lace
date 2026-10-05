# 0006. Instalar e atualizar componentes pela CLI lace

- **Status:** Aceita
- **Data:** 2026-10-03

## Contexto

Instalar um componente do bundle (`lace install`) e procurar versão nova
(`lace update --check`) moram na CLI e no crate `lace-installer`, não no
Core: baixam pacotes da release, conferem o `SHA256SUMS` e desfazem o que
entrou se algo falhar no meio. A AURORA tinha um painel de componentes com
baixar, remover e "doctor".

## Decisão

O Studio roda o `lace` da própria instalação do bundle
(`<instalação>/bin/lace`) com `--json`, como a ADR 0001 do Lace prevê para
interfaces que não usam a biblioteca. A instalação é uma operação como as
do Core: ocupa a vaga de operação, pode ser cancelada e mostra no console
Lace o que a CLI escreve. A conferência dos hashes (`lace tools --verify`) e
a lista de ferramentas usam o Core direto.

## Consequências

- Instalar e atualizar precisam da CLI instalada junto com o bundle (o
  instalador do Lace sempre a põe lá) e de rede.
- Remover componente e reinstalar o Lace continuam fora do Studio: o Lace
  não remove componente, e reinstalar é com o instalador (`lace update`
  sem `--check`, na CLI).
- Se o `lace-installer` ganhar uma API pública, o Studio passa a chamá-la
  direto e esta ADR é substituída.
