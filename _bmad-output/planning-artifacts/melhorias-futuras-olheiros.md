# Melhorias futuras — Olheiros (pós-v1)

> **Implementado em 2026-10-04 (Épico 5, Story 5.1).** As decisões das
> questões em aberto estão na seção "Decisões (2026-10-04)" no fim deste
> documento; o texto abaixo é o registro original das ideias.

> Ideias do Felipe em 2026-10-01, ao aprovar a aba Olheiros (Story 1.4).
> **Fora do escopo do v1.** O v1 segue com o modelo atual: 4
> Especializações × 3 Tiers, custo fixo de contratação
> (`scout::quality::custo_contratacao`) e as 12 combinações sempre
> disponíveis. Este documento guarda as ideias para virar épico/stories
> depois. Ainda não há decisão de design: os números abaixo são os
> sugeridos pelo Felipe, para calibrar depois.

## 1. Identidade do Olheiro

- **Nome personalizado** por Olheiro (hoje o card mostra só Especialização e Tier).
- **Nacionalidade** do Olheiro. Ela interfere:
  - na busca (ver §4, familiaridade com o mercado);
  - no valor de contratação.

## 2. Atributos em estrelas (perfil como o de um jogador)

- A Especialização deixa de ser uma escolha única e vira **atributos**,
  cada um de 0 a 5 estrelas, em passos de 0,5:
  - Caçador de Jovens
  - Caçador de Medalhões
  - Tático
  - Generalista
- Um atributo extra, **Rede de contatos** (0–5 estrelas), afeta a
  **velocidade** de resposta da Missão.
- O tipo de pesquisa pedido na Missão define qual atributo pesa. Um
  Olheiro forte em Caçador de Jovens rende melhor numa Missão de jovens.

## 3. Geração ("spawn") dos Olheiros disponíveis

Hoje as 12 combinações estão sempre à venda. Futuro:

- Os Olheiros oferecidos dependem da **popularidade do clube** e dos
  **títulos conquistados**.
- **Raridade**: Olheiros mais completos ou experientes aparecem com mais
  frequência para clubes de primeira divisão, e bem menos conforme cai o
  nível do clube ou da liga.

## 4. Especialidade de mercado e adaptação

- Cada Olheiro tem **mercados de especialidade** (liga, país ou
  continente; ex.: Premier League, continente africano). A especialidade
  impacta **qualidade** e **velocidade**.
- **Fora do mercado habitual há penalidade**, proporcional à distância
  entre o mercado pedido e a realidade do Olheiro. Exemplo do Felipe: um
  Olheiro da Premier League ou da África mandado mapear a China.
  - **Velocidade** (ex.: Olheiro com 5 estrelas em Rede de contatos):
    cai até **2 estrelas**, em passos de **0,5**.
  - **Qualidade**: cai até **1 estrela**, também em passos de 0,5.
- **Adaptação**: a penalidade é temporária e diminui conforme o Olheiro
  trabalha naquele mercado.
  - Mercado novo: cerca de **6 meses** de carreira para se adaptar.

## 5. Mudança de escopo (tipo de pesquisa)

- Pôr um Olheiro fora do seu foco (ex.: Caçador de Jovens caçando
  medalhão) causa **queda geral de qualidade** até ele se habituar.
- Tempo de readaptação: **6 meses a 1 ano** de carreira, conforme a
  experiência do Olheiro (mais experiente se readapta mais rápido).

## 6. Orçamento da pesquisa (custo da Missão)

- Cada pesquisa tem um **preço associado** para custear viagem e
  hospedagem do Olheiro.
- Quanto maior a escala da pesquisa, maior a **faixa de valores**
  possível.
- O usuário **escolhe a faixa de valor** que quer investir na pesquisa, e
  isso influencia o resultado.

## 7. O que "Qualidade" significa para o Felipe

> "Quando falamos de qualidade estamos falando da quantidade de jogadores
> retornados e a quantidade de falsos positivos."

**Divergência a resolver:** o PRD (Glossário, "Qualidade do Relatório")
define Qualidade em três dimensões: precisão numérica dos atributos,
quantidade de atributos revelados e quantidade de jogadores. A ideia do
Felipe acrescenta **falsos positivos** (jogadores que não atendem de fato
ao filtro). Antes da Story 2.1 (tabela de balanceamento) e da 2.4
(geração de Relatórios), decidir se falso positivo entra já no v1 ou fica
para esta melhoria.

## Questões em aberto (para quando virar épico)

1. Como medir a "distância" entre mercados: mesma liga < mesmo país <
   mesmo continente < outro continente? Usar a lista de ligas/países do
   save?
2. A adaptação progride por dias de carreira (`GJUr.currdate`) ou por
   Missões concluídas naquele mercado?
3. Como as estrelas se combinam com o Tier atual: o Tier some, ou vira um
   resumo das estrelas?
4. Os falsos positivos aparecem no Relatório marcados ("não confirmado")
   ou só se revelam na Ficha?
5. Migração do arquivo de estado: Olheiros já contratados no v1 precisam
   ganhar atributos padrão (campos novos com `#[serde(default)]`, ver
   `scout::persistence`).
6. A nacionalidade e o "spawn" precisam ler mais do save (popularidade e
   títulos do clube): mapear as tabelas antes.

## Stories do v1 que esta melhoria vai tocar

- 1.4 / 1.5: card e contratação (nome, nacionalidade, estrelas, oferta variável).
- 2.1: tabela de balanceamento (penalidades, adaptação, faixa de orçamento).
- 2.2: formulário Nova Missão (faixa de orçamento da pesquisa, aviso de mercado/escopo fora do habitual).
- 2.4 / 2.5: geração e leitura do Relatório (falsos positivos).

## Decisões (2026-10-04)

Felipe pediu para endereçar todas as melhorias; os padrões abaixo foram
escolhidos sem parar para perguntar (dá para recalibrar jogando: tudo
mora em `scout::quality`).

1. **Distância entre mercados:** a liga vale pelo país dela. Mesmo país
   ou continente conhecido = 0; mesmo continente = 1; outro continente
   = 2; busca no mundo todo = 1. Vale a pior região do filtro.
   Penalidade sem adaptação: distância 1 = −0,5★ qualidade e −1★
   velocidade; distância 2 = −1★ e −2★ (os máximos do Felipe).
2. **Adaptação:** por dias de carreira trabalhados naquele lugar,
   calculados das Missões do próprio Olheiro (nada novo no arquivo).
   180 dias zeram a penalidade, em linha reta. Trabalhar um continente
   inteiro conta para os países dele.
3. **Tier:** continua, como resumo das estrelas do foco (até 2,5★
   Júnior, 3–4★ Experiente, 4,5★+ Elite). Multiplica o custo da Missão.
4. **Falsos positivos:** não aparecem marcados no Relatório (as faixas
   continuam contendo o valor real). A Lista de Escolhidos mostra
   "FORA DO FILTRO" quando o acompanhamento chega ao valor exato.
   Baixa: 25% do Relatório; Média: 10%; Alta: nenhum.
5. **Migração:** campos novos com `default`; Olheiro antigo ganha o
   perfil equivalente ao v1 (mesmos números da tabela da Story 2.1),
   sem mercado (= sem penalidade de mercado) e com a Especialização como
   nome. Formato do arquivo v2.
6. **Dados do clube:** `teams.domesticprestige` e
   `internationalprestige` (0–20), divisão da liga e troféus da carreira
   (`career_trophies`, um bit por troféu). No save do Felipe: Barcelona
   20/20, 1ª divisão, 21 troféus → atratividade Alta.

Outras escolhas:

- **Foco e estrelas:** o atributo que vale é o do tipo da Missão; a
  Missão Geral é a do Generalista, e o Generalista (1★ abaixo) serve de
  piso para os outros tipos. Rede de contatos decide o prazo.
- **Fora do foco:** −1★ de qualidade até se habituar (Elite 6 meses,
  Experiente 9, Júnior 12). A Missão Geral e o Generalista nunca têm.
- **Mercado do mês:** 4 a 9 ofertas conforme a atratividade; Elite em
  ~1 de 4 ofertas num clube grande e quase nunca num pequeno; Olheiros
  do país do clube são os mais comuns. Renova a cada mês da carreira;
  quem foi contratado sai da lista do mês.
- **Nome:** gerado pela nação (nomes só com letras que a fonte do
  overlay tem) e editável na confirmação da contratação.
- **Custo de contratação:** ≈ 0,4 M com 2,5★ no foco, 1,4 M com 3,5★,
  4,6 M com 4,5★; Rede, outros atributos e mercados extras somam;
  europeus +10%, sul-americanos +5%, demais −5%.
- **Verba da viagem:** Econômica (60% do custo, ¾ dos nomes, −0,5★,
  faixas ±1 mais largas, um pouco mais lenta), Padrão, Reforçada (160%,
  1¼ dos nomes, +0,5★, um pouco mais rápida).
