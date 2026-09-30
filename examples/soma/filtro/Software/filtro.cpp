#pragma yanc prname filtro
#pragma yanc nuioin 1
#pragma yanc nuioou 1

// Mesma soma no fluxo C: escreve 55 na porta de saída 0.
void main(void)
{
    int acc = 0;
    for (int k = 1; k <= 10; ++k) acc += k;
    out(0, acc);
}
