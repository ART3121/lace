# Circuitos com memória

Até aqui, as saídas dependiam só das entradas do momento. Neste capítulo, os circuitos guardam estado: um registrador guarda o seu valor de uma borda de subida do clock até a seguinte, e só muda na borda. O reset leva os registradores a um valor conhecido, em geral zero, quando o circuito liga ou recomeça. No reset síncrono, o sinal de reset só é olhado na borda do clock, como mais uma entrada de dados. No reset assíncrono, ele age na hora, sem esperar o clock. Nos exercícios, o clock se chama `clk` e o reset, quando existe, se chama `reset` e age com o valor 1.
