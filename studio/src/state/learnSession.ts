// Se o Studio está numa sessão de exercícios do lace learn (state/learn.ts
// entra e sai dela). Fica à parte para o editor e as operações (jobs.ts)
// consultarem sem importar a loja dos exercícios, que importa a delas.
//
// Na sessão, nada grava sozinho ao trocar de aba, e nenhuma operação revela
// o painel: os consoles só ganham o ponto de saída nova.

let active = false;

export const learnSession = {
  active: (): boolean => active,
  set: (value: boolean): void => {
    active = value;
  },
};
