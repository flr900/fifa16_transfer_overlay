# Integração da Central de Scout com o scout nativo do FIFA 16

> Mapeamento de 2026-10-05, a pedido do Felipe. **Só análise estática dos
> saves em disco** (nada foi testado com o jogo aberto, nada foi
> implementado). Save usado: `64886395` (PSG, 2029, FIFA Friends).

Pedidos:

1. Terminado um scout na Central, o jogador entra na **Lista de Escolhidos
   nativa** e tem os **atributos liberados no FIFA**, como num scout
   nativo; com níveis de detalhe (no mínimo valor, salário e contrato já
   revelados na lista). E-mail nativo com o relatório: menos prioritário.
2. **Valor e salário determinísticos**, sem inferência.

## 1. O que o save revela sobre o scout nativo

O scout do FIFA 16 chama-se **GTN (Global Transfer Network)**. Ele **não
está** onde se esperava:

| Onde | O que tem | Conclusão |
|---|---|---|
| `career_scouts` (`zlrC`), `career_scoutmission` (`apoo`) | Schema existe (scout com `knowledge`, `experience`, `regionid`, `state`; missão com `playertype`, `nationality`, `durationid`, `returningdate`) | **0 linhas em todos os 7 saves** e nos 3 backups, embora o save tenha relatórios GTN. Não guardam o estado que importa. |
| Tabelas t3db (34 de carreira) | Nenhuma tabela de lista de observação ou de "jogador já observado" | O estado de "Escolhidos" e de atributos revelados **não é uma tabela**. |
| **Cauda não-t3db do `DATA`** (≈1,8 MB depois do 3º banco, a partir de ~8,6 MB) | Caixa de e-mail serializada: 34 e-mails, entre eles `CM_Email_GTN_FinalPlayerReport_Subject` ×5, `CM_Email_GTN_PlayerReport_Body_Complete` ×5, `CM_Email_GTN_AreaReport_*` ×4, com o `playerid` em binário ao lado (ex.: `0x113AB` = 70571 ao lado de "Paul Jakupović") | **O GTN do Felipe existe e deixa rastro aqui.** É um formato de objetos serializados (chaves de texto + ints little-endian), não t3db. |
| Mesma cauda, `CM_View_Shortlist` | É só o rótulo do **botão** de um e-mail ("ver lista"), não a lista | A lista de observação em si **ainda não foi localizada**. |

Pontos a esclarecer com um experimento (ver §3): onde mora "este jogador
foi observado / com que nível", e onde mora a lista de Escolhidos.

### Como escrever isso no jogo (a parte difícil)

- Escrever no `DATA` está fora de questão (checksum; sessões 3 e 4).
- Escrever em memória funcionou **só** para campos de carreira simples e
  contíguos (`transferbudget`, sessão 4). Atributo por jogador falhou
  (buffers de UI que se reciclam).
- Lista de Escolhidos, "observado" e caixa de e-mail são **coleções de
  objetos do motor**: inserir um item exige alocar/encadear objetos
  (strings localizadas incluídas), não só gravar 4 bytes. É provável que
  precise **chamar funções do próprio jogo** a partir da DLL (já injetada),
  o que significa engenharia reversa num `fifa16.exe` com packer.
- Risco: corromper a lista e o save. Sempre com backup e compare-and-write.

## 2. Valor e salário: o que dá para fazer

### 2.1 Diagnóstico da estimativa atual (`scout::quality::valor_estimado`)

Medido contra `career_transferoffer.valuation` (`QWbR`), o **valor de
mercado que o próprio jogo calculou** ao fazer a proposta. 169 jogadores
com `valuation` no save:

| Estimador | Mediana estimado/real | Erro médio (log) | Dentro de ±20% |
|---|---|---|---|
| Atual (3 M × 1,2^(OVR−70) × idade × crescimento) | 1,29× | 0,44 | **34%** |
| Regressão em OVR, potencial e idade (melhor caso simples) | 1,0× | 0,26 | **44%** |

O erro é sistemático nos extremos: OVR 57 estimado em ~510 mil, o jogo diz
90 mil; OVR 93 estimado em 207 M, o jogo diz 85,5 M. **Nenhuma fórmula
simples sobre OVR/potencial/idade passa de ~45% dentro de ±20%** — o valor do
FIFA depende de mais coisas (clube/liga, contrato restante, posição,
fator de mercado), e a fórmula está dentro do executável. Ou seja:
**recalibrar não resolve o "determinístico"**, só reduz o erro.

Salário: `career_playercontract` (`DvsP`) tem o `wage` **real** só dos
jogadores do **próprio elenco** (31 linhas). `salary_demand` vem 0.
`career_transferoffer.offeredwage` é oferta, não o salário do jogador.

### 2.2 Caminhos, do mais ao menos viável

| # | Caminho | Cobre | Determinístico? | Esforço / risco |
|---|---|---|---|---|
| A | **Ler o que o jogo já calculou**: `valuation` e `offeredfee` das propostas (`QWbR`), `wage` do elenco (`DvsP`), contrato real (`contractvaliduntil`, já usado) | Elenco próprio + quem recebeu proposta (~170 de 39 mil) | **Sim**, para esses | Baixo; só leitura do save. Usar como valor "real" quando existir, estimativa só no resto. |
| B | **Usar A como base de calibração**: ajustar por faixa (OVR × idade × liga) com mais amostras (juntar propostas de vários saves) e mostrar "≈" | Todos | Não, mas erro menor | Baixo; teto de ~45–60% dentro de ±20% com os dados atuais. |
| C | **Capturar o valor/salário que a tela do jogador mostra** (ler o buffer de UI quando o Felipe abre o perfil, como a sessão 4 já achou 18 endereços ligados ao jogador) | Só quem o Felipe abrir | Sim | Médio; buffers efêmeros, precisa de sonda por tela e "aprender" os endereços a cada sessão. |
| D | **Chamar a função de valor do jogo** a partir da DLL | Todos | **Sim** | Alto: reverso do `fifa16.exe` com packer; o mesmo tipo de trabalho que resolveria a escrita (itens do §1). |

Recomendação: **A agora** (barato, e já elimina o erro justo onde o
Felipe mais negocia: o próprio elenco e propostas), **B** para o resto
com "≈" mais honesto, e tratar **D** como o mesmo projeto de engenharia
reversa da integração com o scout nativo (§3, fase 2), já que as duas
metas dependem de chamar funções do jogo.

## 3. Plano proposto

### Fase 0 — Experimento controlado (~1 sessão, sem código novo no overlay)

Objetivo: descobrir onde o scout nativo grava o quê. O Felipe faz, no
jogo, com saves de antes/depois:

1. Salvar (A) → pôr **um** jogador na lista de observação → salvar (B).
   Diferença A×B na cauda do `DATA` mostra onde mora a lista.
2. Salvar (B) → mandar uma missão GTN e avançar até o relatório → salvar
   (C). B×C mostra o estado de "observado", o nível de detalhe e a
   estrutura do e-mail (inclusive o `playerid`).
3. O mesmo A/B/C **na memória viva** (`scan_value_live` com o `playerid`
   conhecido), para ver se a lista tem um array simples de ids (cenário
   bom) ou objetos encadeados (cenário difícil).

Entregável: layout documentado da lista, do "observado" e do e-mail, e a
decisão go/no-go da Fase 2 com base em quão simples é a estrutura viva.

**Ferramenta (2026-10-06):** `fifa_process_identifier/scout_probe.py`,
somente leitura. `capture <rótulo> --int <valor>` guarda onde estão, na
memória PRIVATE, os int32 pedidos (playerid, data de retorno da missão) com
contexto, e onde estão as strings `CM_Email_*`/`CM_View_*` (caixa de
entrada viva); `diff A B` lista o que apareceu, sumiu e mudou, com os
vizinhos de cada ocorrência nova (uma lista de ids tem vizinhos que são
ids). Capturas ficam em `probe_runs/` (fora do git). `selftest` valida a
sonda sem o jogo.

**Roteiro no jogo** (carreira de teste "Senhor Manager", save `65aa3ab0`,
01/07/2026, clube 243 = Real Madrid — o Felipe autorizou perder este save; carregada no
hub; jogador de teste **Sverre Nypan, playerid 268737**, meia norueguês de 19
anos, fora do elenco e sem proposta no save). Onde o roteiro diz 284728,
usar 268737:

1. `capture T0 --int 284728`; salvar o jogo.
2. Transferências → buscar "Pozzo" → pôr na lista de observação. Voltar ao
   hub. `capture T1 --int 284728`; salvar o jogo. `diff T0 T1`.
3. Tirar da lista. `capture T2 --int 284728`. `diff T1 T2`: o que some é o
   endereço da lista.
4. Mandar uma missão GTN; anotar a data de retorno. `capture T3 --int
   <AAAAMMDD>`.
5. Avançar até o relatório chegar; abrir o e-mail. `capture T4`; salvar o
   jogo. `diff T3 T4`: onde nasceu o e-mail e o que o rodeia.

### Fase 1 — Valor/salário "do jogo" (sem tocar no jogo)

- `valor_real_do_jogo(playerid)` a partir de `QWbR.valuation`/`DvsP.wage`,
  com origem marcada na tela ("do jogo" × "≈ estimado").
- Reaproveitar nos Escolhidos: valor, salário e contrato aparecem como
  **exatos** quando o jogo os tem. Quando não, "≈".
- Calibração B reaproveitando os mesmos dados (um teste `--ignored`, como
  a calibração do Fit Posicional).

### Fase 2 — Escrever no scout nativo (depende da Fase 0)

Em ordem de valor/risco, parando onde a estrutura ficar cara:

1. **Lista de Escolhidos nativa:** adicionar o jogador à lista do jogo ao
   terminar o scout (se for lista de ids, é o item mais barato).
2. **Níveis de detalhe:** mapear o nível nativo e mapear
   Qualidade Baixa/Média/Alta da Central para ele. Mínimo pedido: valor,
   salário e contrato revelados ao entrar em Escolhidos.
3. **Atributos desbloqueados:** marcar o jogador como "observado" no nível
   mapeado.
4. **E-mail (menor prioridade):** inserir o relatório GTN na caixa. É o item
   mais caro (objeto + strings localizadas); alternativa barata: a Central
   mostrar o próprio "e-mail" no overlay, sem tocar no jogo.

Todo passo de escrita: backup do save, compare-and-write com leitura de
volta, desligável por configuração (a Central nunca depende da escrita).

## 3b. Resultados do experimento (2026-10-06, carreira de teste)

**Comprovado em jogo** (scripts em `fifa_process_identifier/`, todos
precisam de terminal elevado; compare-and-write com releitura):

1. **Lista de Escolhidos nativa** = vetor na memória (ponteiros início/fim/
   fim-da-capacidade de 8 bytes numa estrutura dona), entradas de 28 bytes
   `{team i32, playerid i32, -1 ×4, flag u8}`, capacidade de **100**. No save
   é um bloco `sl003` (cabeçalho de 30 bytes com o contador) + uma entrada
   `sl004` de 31 bytes por jogador. `scout_shortlist.py --add` acrescentou o
   Kostoulas pela memória e ele apareceu na lista do jogo. O `team` é o time
   do jogador quando foi adicionado.
2. **Conhecimento do jogador** = vetor ordenado por playerid de registros
   de 20 bytes `[playerid, a, b, aaaammdd, -1]` (capacidade 1500, 29 em uso
   no início). O `b` (0–198) é o nível: pedir observação ao scout soma +70 a
   cada 3 dias (70, 140, 198). Com `b` baixo já há estimativas de atributos;
   `b ≥ 140` mostra taxa de transferência e salário e refina as estimativas;
   `b = 198` mostra todos os atributos exatos. Escrever no `b` de um registro
   existente (`scout_poke.py`) e **inserir** um registro novo ordenado
   (`scout_insert.py`: abre espaço, grava, avança o ponteiro de fim) liberou o
   que se esperava, sem scout nenhum trabalhando. O jogo gravou isso no save
   (contador do cabeçalho recalculado sozinho) e a recarga manteve.
3. **Valor de mercado exato**: o jogo calcula o valor de todo jogador que a
   tela de busca mostrou e deixa numa linha de cache da UI
   (`[id, time, …, valor, nome]`; Nypan = 5.000.000 antes de qualquer
   revelação). Fonte determinística candidata para o `valor_estimado`.
4. **Pedir observação** grava: o registro de conhecimento (acima), um
   registro na fila de pedidos (seção `mm002`: contador, data, tipo 2,
   scout 1) e e-mails; nenhuma tabela t3db muda; `career_scouts` e
   `career_scoutmission` ficam vazias.

**Ainda aberto:** significado do `a` (`16<<16`, `6<<16`, 0/1/2); em qual `b`
aparece cada informação (atributos × valor × salário × contrato) para mapear
Qualidade Baixa/Média/Alta; achar os ponteiros pelo jogo sem escanear 1,9 GB
(hoje 20–25 s por varredura, e o dono do vetor muda de lugar ao recarregar o
save); persistência da entrada de escolhidos no save; e-mail nativo (menor
prioridade); como evitar que o jogo reescreva o `b` no próximo estágio de 3
dias (observação de fundo).

## 4. Decisões para o Felipe

- Topar a **Fase 0** (o experimento A/B/C no jogo)? Sem ele, não há como
  estimar o esforço da Fase 2.
- **Fase 1** pode começar já, em paralelo, porque não depende do jogo.
- Aceitar que o valor "exato" só existe onde o jogo já o calculou, até a
  engenharia reversa (D) acontecer?
