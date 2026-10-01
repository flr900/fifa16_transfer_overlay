# Melhorias futuras — Olheiros (pós-v1)

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
