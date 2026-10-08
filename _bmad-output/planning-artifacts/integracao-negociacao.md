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

## Sinal de tela encontrado (gravado em 2026-10-08, build 7.6-v29)

O ponteiro `fifa16.exe+0x3357378` aponta para uma tabela de **16 entradas fixas
de 64 bytes** (`0x2BD92000`...`0x2BD923C0` nesta sessão). Não é um anel que
gira: cada entrada tem um papel. A entrada **`0x2BD92100`** guarda o **nome do
último evento de tela**; as outras guardam o último widget (`.swf`), o último
item do noticiário ("recentemente do ... por $ ...") e dicas de tela. Tudo muda
**70 a 200 ms depois** do aperto que causou a mudança.

| Momento | Texto na entrada `...2100` |
|---|---|
| Hub | `MainMenuHub`, depois `CacheTeamSheet` (hub carregado) |
| `A` no bloco de Escolhidos | `ViewShortlist` (entrou), `NotifyScreenLoadedAndRefresh` (~1,3 s: lista pronta) |
| `A` num jogador da lista | `ActionPopup` (menu do jogador aberto) |
| Compra | `EnterTransferOfferActionPopup`, `TransferOffer` |
| Contrato | `EnterPreContractOfferFromActionPopup`, `ContractOffer` |
| `B` da negociação | `SendReadyOnLoadComplete`, depois `ActionPopup` (~0,7 s) |
| `B` do menu | `NotifyScreenLoadedAndRefresh` (volta à lista) |
| `B` da lista | `ViewShortlist`, `MainMenuHub`, `CacheTeamSheet` |

- **Compra e empréstimo são a mesma tela** (`TransferOffer`); `RB` troca para a
  aba de empréstimo e `LB` volta. A troca de aba **não gera evento** na entrada
  `...2100`. A tela aberta pela opção de empréstimo não volta para a compra.
  Então, para o empréstimo, o caminho previsível é abrir pela **compra** e dar
  `RB`.
- **"Pre-contract"**: o evento de contrato é `PreContractOffer`; a opção
  "Negociar para assinar contrato" provavelmente só existe para quem está no
  fim do contrato. Isso explicaria por que o menu varia.
- **As opções do menu não aparecem em nenhuma entrada** (os textos de dica e do
  noticiário não são os rótulos). Não dá para saber quais opções o jogador tem.
- O foco do jogo (`foco no jogo: <id>`) atualiza ao abrir o menu (`ActionPopup`),
  como já visto.

Consequência para o roteiro: cada passo pode ser **conferido pelo evento
esperado** (e abortado com `B` se vier outro evento ou nenhum em ~1 s), mas
escolher a opção continua às cegas dentro do menu.

## Fase 1: roteiro "Abrir no jogo" (build 7.6-v30, 2026-10-08)

Implementado, ainda **sem teste em jogo**. `telas.rs` lê o evento de tela (acha a
entrada de eventos pelo vocabulário, porque o endereço muda a cada sessão),
`scout/roteiro.rs` é a máquina de estados e o gancho do XInput (`gamepad::injetar`)
aperta os botões para o jogo; o controle de verdade fica de fora enquanto roda,
e `Select` cancela.

Decisões:

- **Parte da lista de Escolhidos do jogo já aberta.** No hub está o bloco
  "Avançar" (`FluxTile_Advance`) e um `A` errado avançaria o calendário.
  A lista conta como aberta depois de `ViewShortlist` (sem ter voltado ao hub).
- **Varre a lista**, uma linha por vez: `A`, espera `ActionPopup`, espera o foco
  mostrar o jogador (~0,8 s), compara com o alvo; se não for, `B`, espera a lista
  voltar (`NotifyScreenLoadedAndRefresh`) e `↓`. Para com o menu do alvo aberto.
  A ordem da lista não precisa ser conhecida.
- **Paradas seguras**: evento esperado que não chega no prazo, jogador em foco
  ilegível, a mesma linha duas vezes (lista sem volta), lista inteira sem o
  jogador, painel aberto ou `Select`. Nenhum botão sai fora da lista.
- Só para quem está na lista do jogo (`no_jogo` ou `importado`).

Falta (próximas fases): escolher a opção do menu (contrato, compra, empréstimo via
`RB` na tela de compra), acelerar a busca (hoje ~1,2 s por linha), entrar na lista
a partir do hub, e gerenciar negociações.

## Caminho direto: etapa de leitura (build 7.6-v32, 2026-10-08)

O roteiro de botões funciona, mas leva ~3 s por linha da lista do jogo (minutos
no pior caso). A alternativa é **disparar o evento que abre a tela** (por exemplo
`EnterTransferOfferActionPopup`) sem apertar botões. Isso exige achar, no código do
jogo, a função que despacha esses eventos e o que ela recebe. Plano em etapas,
**a primeira só de leitura**:

1. **Despejar a imagem** do `fifa16.exe` da memória (o executável tem packer; em
   memória o código já está desempacotado). `despejo.rs`: com o arquivo
   `%TEMP%ifa_despejar_imagem.pedido`, o overlay grava `%TEMP%ifa16_imagem.bin`
   (deslocamento = RVA) e `fifa16_imagem.json`. Só leitura, thread própria, em blocos.
2. **Analisar offline** (`fifa_process_identifier/analisar_imagem.py`, sem dependências):
   `secoes` (o código está desembaralhado?), `strings`, `xrefs` (quem faz
   `lea reg,[rip+x]` para a string do evento), `chamadores` (quem chama a função).
   Testado em `python314.dll`.
3. Só se a análise mostrar uma função clara: **gancho passivo** que registra os
   argumentos enquanto o Felipe faz o fluxo à mão (como o gravador faz com os botões).
4. Por último, chamar a função a partir da Central.

Incógnitas: se o código da imagem despejada está desembaralhado; se o evento recebe
só o nome ou também o contexto do jogador selecionado.

### Resultado da etapa de leitura (2026-10-08)

Despejo: 156.389.376 bytes (`SizeOfImage 0x9524000`), base `0x140000000` (**sem ASLR**: os
endereços do executável são os mesmos a cada sessão), 986 ms, nenhuma página ilegível.
Os nomes das seções são ofuscados (`.data` executável com 34 MB zerado, `.tls em` com
95 MB). **O código real está desempacotado em memória**: a seção `.tls em` (RVA
`0x39FC000`) tem entropia 6,6 e ~25% de bytes de código; as strings dos eventos estão em
`.xpdata` (o `.rdata`).

O que o código mostra:

- Os eventos de tela são **Actions de uma máquina de estados (FSM)**, construídas por um
  construtor gigante (`~0x59F5040`, classe `CareerModeInitModeStateMachine...`, vtable RVA
  `0x3068590`). Cada Action é só um **token nomeado**: `[0]` vtable (RVA `0x30A8BF8`),
  `[8]` ponteiro do nome, `[0x10]` nome da máquina dona (`CareerModeStates`). Fica num campo
  do objeto da máquina (ações em `+0xe80`, `+0xe98`...). Há também um **mapa nome → Action**
  (`operator[]` em RVA `0x4274EC0`, string em `0x3A20E80`).
- Ações relevantes (`+offset` no objeto da máquina): `ActionShowMyActions` (`+0xe80`),
  `ActionEnterTransferOfferActionPopup` (`+0xe98`, compra),
  **`ActionEnterLoanOfferActionPopup` (`+0xeb0`, empréstimo: ação própria, sem o `RB`)**,
  `ActionEnterPlayerContractNegotiationFromActionPopup` (`+0xec8`),
  `ActionEnterContractOfferFromActionPopup` (`+0xee0`),
  `ActionEnterPreContractOfferFromActionPopup` (`+0xef8`).
- A Action não tem método de execução (a vtable tem só destrutor e um método de tipo):
  quem a executa é a máquina de estados.
- A instância da máquina **não está na imagem estática** (nenhuma ocorrência da vtable no
  despejo): é alocada no heap.

Ainda não achado: a função que posta uma Action na máquina, e onde fica o "contexto do
jogador selecionado" que a ação `...FromActionPopup` consome.

Próximo passo (só leitura): achar a instância da máquina na memória viva (varrer o heap pelo
valor da vtable `0x143068590`) e **observar** os campos dela enquanto o Felipe faz os fluxos,
para ver se há um campo de "estado atual" ou "ação pendente". Se houver um campo simples,
escrevê-lo seria o mesmo tipo de escrita que já funcionou no orçamento, sem chamar função.

Ferramentas: `fifa_process_identifier/re_desmontar.py`, `re_usos_desloc.py`, `re_refs_rip.py`,
`re_vtable.py` (usam o despejo de `%TEMP%` e o `capstone`).

### Observação da máquina de estados (builds 7.6-v33 a v37, 2026-10-08)

A máquina de estados do modo carreira (objeto de ~8 KB+ no heap, 204 Actions de `+0x28` a
`+0x1348`, vtable RVA `0x304BFE8`) foi achada pelo ponteiro do nome da Action de compra
(`[Action+8]`, `+0xE98`) e conferida pelos vizinhos (empréstimo `+0xEB0`, negociação
`+0xEC8`). A vtable das Actions é sobrescrita pela do tipo de cada uma; só o nome serve de prova.

O que a observação com amostrador de 1 ms mostrou:

- O cabeçalho é uma **caixa de correio de uma posição**: `+0x10 = 1` e `+0x18 = ponteiro
  da mensagem` por ~18 a 28 ms, e a máquina consome (volta a 0). Mensagens têm a vtable
  `RVA 0x308D2E8` (classe "Mensagem", referenciada em dezenas de pontos do código: é a fila
  genérica de eventos da interface).
- `B` gera um **par** de mensagens pequenas (começo e fim de transição, ~700 ms entre elas)
  e o campo `+0x13A0` vai `0x1300 -> 0x1301 -> 0x1300`. `A` numa opção do menu gera **uma**
  mensagem grande.
- A mensagem de `A` tem contadores de sequência (`+0x8..+0x14`, que mudam de uma vez para
  outra) e uma carga com **um nome de ação** (`"ActionP..."`, cortado pelo despejo). A de compra e
  a de empréstimo diferem nas listas de ponteiros da carga, não no cabeçalho.
- **Nenhum campo da máquina muda com a tela aberta** (compra, empréstimo): a máquina
  recebe eventos da interface; a escolha da ação nasce na camada acima (Flash).

Conclusão: não há um campo simples de "ação pendente" para escrever. Disparar uma tela direto
exigiria construir uma mensagem válida (classe, carga, alocação) e conhecer o contexto do
jogador selecionado, ou achar e chamar a função do menu. Risco de derrubar o jogo alto, ganho
incerto. **Linha pausada.** O que resta de útil: o roteiro de botões com salto por ordenação
por nome.

## Fora do escopo por ora

O limite de slots de Olheiros foi adiado a pedido do Felipe (2026-10-08): o
impacto no equilíbrio ainda não está claro. O e-mail nativo (7.7) fica
depois desta integração.
