# Integração da Central de Scout com a negociação do jogo

> Pedido do Felipe, 2026-10-08, antes de atacar o e-mail nativo (7.7).
> Só análise e uma ferramenta de gravação; nada disto foi testado em jogo ainda.

## Pedido

A partir da Central, abrir a tela de negociação do jogo para um jogador, nos
três modos que o FIFA tem:

| Modo | O que abre no jogo |
|---|---|
| **Conversar sobre compra** | negociação de compra |
| **Conversar sobre empréstimo** | negociação de empréstimo |
| **Negociar contrato** | a negociação já na oferta salarial |

A tela de negociação tem a parte de salário e a de compra/empréstimo. Além de
abrir a tela, a Central também deveria **gerenciar negociações** (ver o que
está em andamento) e levar direto a elas.

## O que já existe e ajuda

- **Dá para mandar botões ao jogo.** O overlay já intercepta
  `XInputGetState` (`gamepad.rs`, `desviar`) e devolve ao jogo o controle
  "parado" com o painel aberto. O mesmo ponto pode devolver um estado
  **sintetizado**: um roteiro de apertos (D-pad, A, B...) que o jogo lê como
  se fosse o controle real.
- **Dá para saber onde o foco está.** A linha de valor do jogador em foco
  (Story 7.6, `save_repo/foco.rs`) é lida a cada 0,5 s e traz id e nome. Isso
  permite andar numa lista até o jogador certo e **conferir antes de apertar A**.
- **Dá para pôr o jogador numa lista do jogo.** A Central já escreve na lista
  de escolhidos nativa (Épico 7). Chegar num jogador dentro dessa lista é bem
  mais confiável do que digitar o nome numa busca com o controle.
- **O jogo tem eventos de tela com nome** (`EnterTransferOfferFromInbox`,
  `EnterScoutReport`...; ver `MEMORY_INVESTIGATION_NOTES.md`). Chamá-los
  direto seria o caminho limpo, mas é engenharia reversa do `fifa16.exe` com
  packer, o mesmo trabalho de alto risco já descartado para a escrita de
  atributos.
- **Propostas ficam no save.** A tabela `career_transferoffer` (`QWbR`) tem
  `playerid`, `teamid`, `offerteamid`, `isloan`, `stage`, `result`,
  `offeredfee`, `offeredwage`, `valuation`. Dá para listar negociações, mas só
  como estavam **no último save** (a mesma limitação do elenco).

## Abordagens

| | Como | Prós | Contras |
|---|---|---|---|
| **A. Roteiro de botões** (recomendada) | A Central fecha o painel, solta o controle ao jogo e aplica a sequência de apertos até a tela; confere o jogador em foco no caminho | Reaproveita o gancho que já existe; sem engenharia reversa; reversível | Frágil a mudança de menu; depende de saber o ponto de partida e o tempo de cada tela |
| B. Chamar funções do jogo | Disparar o evento de tela direto da DLL | Instantâneo e exato | Engenharia reversa de alto risco (packer); pode derrubar o jogo |
| C. Mouse/teclado | `SendInput` em coordenadas | Dispensa o controle | Depende de resolução e layout; pior que A |

## Plano em fases

0. **Gravar os caminhos** (feito, ferramenta pronta: build `7.6-v26`). O
   overlay passa a registrar no log cada aperto e cada mudança do jogador em
   foco. O Felipe faz cada fluxo uma vez e a sequência exata sai do log.
1. **Reproduzir um fluxo**, o mais simples (provavelmente "Negociar contrato"
   ou "Conversar sobre compra" a partir da lista de escolhidos): um roteiro
   curto, com confirmação do foco, desligável por configuração. Medir
   confiabilidade.
2. **Os três modos** a partir de um botão "Negociar" na Ficha (e nos
   Escolhidos), com as regras de partida (jogador na lista do jogo; o que fazer
   se o jogo não está no hub).
3. **Gerenciar negociações**: aba ou tela com as propostas do `career_transferoffer`
   (em andamento, aceitas, recusadas; compra ou empréstimo), com "ir para a
   negociação". Dado do último save.
4. **Melhorar a fonte das negociações**: achar a lista viva em memória (como a
   lista de escolhidos), se a leitura do save for lenta demais.

## Perguntas abertas

- De onde sai cada fluxo hoje no jogo (hub → Transferências → ...)? E o que
  muda com o jogador fora da lista de escolhidos? (Fase 0 responde.)
- Como saber que uma tela terminou de carregar antes do próximo aperto?
  Hipótese: tempo fixo mais o foco no jogador; talvez um sinal de tela na memória.
- E se o jogador estiver em outro clube, emprestado, ou o mercado fechado?
  O roteiro precisa abortar com uma mensagem clara.
- A Central pode ser usada com teclado/mouse no jogo, ou só controle? O
  roteiro só cobre controle.

## Fase 0: como gravar

1. Rodar `fifa_overlay\recarregar_dev.ps1` (build `7.6-v26`).
2. Criar o arquivo `%TEMP%\fifa_gravar_controle.pedido` (vazio). O gravador
   liga em até 1 s e escreve `[gravador]` no `%TEMP%\fifa_overlay.log`.
3. No jogo, fazer **um fluxo por vez**, com uma pausa de uns 5 s entre eles,
   sempre começando de um ponto conhecido (por exemplo o hub), e anotar a
   ordem: (a) conversar sobre compra, (b) conversar sobre empréstimo,
   (c) negociar contrato. Se possível, o mesmo jogador, que esteja na lista
   de escolhidos do jogo.
4. Apagar o arquivo para parar.

O log traz `+A` / `-A` (apertou/soltou), D-pad, gatilhos, analógico esquerdo
e `foco no jogo: <id> "<nome>"`, todos com tempo em ms.

## Resultado da Fase 0 (gravado em 2026-10-08, jogador de teste: Mbappé)

Cadência dos apertos: ~70 ms apertado, ~90 a 130 ms entre eles.

| Passo | Sequência |
|---|---|
| Hub → lista de Escolhidos | no hub, selecionar o bloco de Escolhidos (o cursor do hub **lembra o último bloco**: na 1ª vez foi o analógico + `A`; depois, `→` + `A`) |
| Achar o jogador na lista | `↑` ×10 a partir do topo (a lista dá a volta), depois `A` abre o **menu do jogador** |
| Menu do jogador | itens: **Adicionar/remover escolhido**, **Perguntar sobre &lt;nome&gt;**, **Negociar para assinar contrato**, **Conversar sobre compra**, **Conversar sobre empréstimo** |
| Opção | cursor volta ao topo: contrato `↓` ×2, compra `↓` ×3, empréstimo `↓` ×4, depois `A` |
| Voltar | `B` ×5 da negociação até o hub; `B` ×1 volta ao menu do jogador |

Descobertas que mudam o desenho:

- **O jogador em foco só atualiza quando o menu do jogador abre**, não enquanto
  se anda na lista (ficou parado em outro jogador por 17 s). A conferência
  "é o jogador certo?" vem **depois** do `A`; se errou, `B` e corrige.
- **A ordem da lista na tela não é a da memória** (Mbappé: índice 75 de 81 na
  memória; dez `↑` com a volta dão a posição 71 na tela). Por padrão a lista
  é ordenada por **posição**, e o jogador a reordena por nome, posição ou valor
  (`→` até a coluna e `X`). Ordenar **por nome** daria uma ordem que a Central
  sabe calcular.
- **O menu varia**: quem não tem negociação de salário não tem a opção de
  contrato, e há jogadores que não dá para abordar. Contar `↓` às cegas pode
  cair numa opção errada e, no pior caso, em "Adicionar/remover escolhido",
  que **tira o jogador da lista**.
- **O hub não tem posição fixa**: o roteiro precisa saber em que tela está.

Conclusão: o que falta para o roteiro ser seguro é um **sinal de tela** (em que
tela o jogo está, e qual item do menu está selecionado). Sem isso, só dá para
fazer a parte "levar até o jogador" com a condição de que o jogador já esteja
na lista de Escolhidos do jogo.

### Fase 1A: procurar o sinal de tela
`fifa_process_identifier/scout_probe.py capture <rótulo> --str "<texto>"`
(novo, 2026-10-08) procura textos da interface na memória, em ASCII e UTF-16,
e `diff A B` mostra os que mudaram de quantidade entre duas telas. Os textos
carregam sob demanda e podem ficar na memória depois que a tela fecha; o
experimento diz se a quantidade ou o endereço muda com a tela aberta.

## Fora do escopo por ora

O limite de slots de Olheiros foi adiado a pedido do Felipe (2026-10-08): o
impacto no equilíbrio ainda não está claro. O e-mail nativo (7.7) fica
depois desta integração.
