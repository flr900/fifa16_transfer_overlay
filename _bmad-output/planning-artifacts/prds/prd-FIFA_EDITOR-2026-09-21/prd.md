---
title: Central de Scout — FIFA 16 Companion
status: final
created: 2026-09-21
updated: 2026-09-21
---

# PRD: Central de Scout — FIFA 16 Companion
*Working title — confirmar.*

## 0. Documento — Propósito

Este PRD descreve a **Central de Scout**, uma nova funcionalidade do FIFA 16
Companion (o "overlay" já existente, ver `PROJECT_MEMORY.md`). É dirigido ao
próprio Felipe, como PM e desenvolvedor único do projeto, servindo de
referência para as próximas sessões de implementação (`bmad-create-epics-and-stories`,
`bmad-architecture`). O vocabulário do Glossário (§3) é vinculante — features e
requisitos funcionais (FRs) usam esses termos literalmente. Suposições
inferidas durante a elicitação estão marcadas inline com `[ASSUMPTION]` e
indexadas em §9.

Este PRD assume como contexto técnico prévio tudo que já está documentado em
`PROJECT_MEMORY.md` (parser do save, pipeline de leitura in-process via DLL,
escrita bem-sucedida de `transferbudget`, limitações de escrita de atributos
de jogador) — não duplica esse conteúdo aqui.

## 1. Visão

O FIFA 16 tinha, nativamente, um sistema de scouting raso: escolher um país
e receber uma lista de prospects sem qualquer filtro por atributo ou posição
real. A comunidade de Football Manager já provou, há mais de uma década, que
scouting pode ser uma mecânica profunda e satisfatória — mas o próprio FIFA
16 nunca chegou perto disso, e a EA depois *removeu* até o pouco que existia
em versões seguintes.

A Central de Scout é a resposta do Companion a essa lacuna: uma central de
recrutamento paralela ao jogo, acessível dentro do overlay que já roda por
cima do FIFA 16 em fullscreen, sem tirar o jogo da tela. Nela, o usuário
contrata olheiros com diferentes especializações e níveis de habilidade,
encomenda pesquisas com filtros muito mais ricos do que qualquer coisa nativa
do FIFA (perfil de atributos, fit fora de posição, cobertura geográfica
multi-seleção, jogador de referência do elenco), e recebe relatórios cuja
qualidade — precisão dos números, atributos revelados, quantidade de nomes —
varia de acordo com o olheiro escolhido e o tempo investido.

O valor central: transformar a janela morta de "esperar a carreira avançar"
em um loop de descoberta ativo e estratégico, sem exigir que o jogo em si
seja modificado ou que o save seja escrito de forma arriscada. A Central de
Scout **lê** o estado real da carreira (orçamento, elenco, data da carreira)
e **escreve apenas** o campo já validado como seguro (`transferbudget`) —
tudo o mais (olheiros contratados, missões em andamento, relatórios gerados)
vive inteiramente dentro do Companion, como uma camada de simulação paralela.

## 2. Usuário-Alvo

### 2.1 Jobs To Be Done

- Como jogador de Modo Carreira do FIFA 16, quero descobrir jogadores que se
  encaixem no meu time (por perfil técnico, não só por overall bruto) sem
  precisar vasculhar manualmente centenas de jogadores na busca nativa.
- Como jogador que já mapeou os limites da busca nativa de transferências,
  quero filtros que a EA nunca ofereceu: fit fora de posição, jogador
  "parecido com o meu titular X", cobertura geográfica combinável.
- Como usuário deste projeto (Felipe, também desenvolvedor), quero uma
  funcionalidade que valide se o investimento em overlay + leitura de memória
  in-process (sessão 5) compensa, antes de aprofundar mais engenharia reversa
  de escrita de atributos de jogador.
- Como jogador que gosta da progressão de longo prazo do Modo Carreira, quero
  que contratar/manter olheiros tenha um custo real (orçamento do clube),
  para que a decisão de investir em scouting seja uma escolha estratégica
  genuína, não um cheat sem custo.

### 2.2 Não-Usuários (v1)

- Jogadores de outras versões do FIFA/EA FC (17+) — o parser e a leitura de
  memória são específicos da build do FIFA 16 (`16.0.2904053`).
- Usuários que querem que o Scout efetivamente **contrate** jogadores dentro
  do save (ex: uma transferência real sendo processada pelo motor do jogo) —
  isso é não-objetivo explícito do v1 (ver §5).
- Usuários em modo online/multiplayer (Ultimate Team, Pro Clubs) — o escopo é
  inteiramente Modo Carreira single-player.

### 2.3 Jornadas-Chave do Usuário

- **UJ-1. Felipe monta um olheiro tático para reforçar o meio-campo.**
  - **Persona + contexto:** Felipe está numa carreira avançada, precisa de um
    volante mas o orçamento é apertado; já esgotou os nomes óbvios na busca
    nativa do FIFA.
  - **Estado inicial:** FIFA 16 rodando em Modo Carreira, fullscreen. Felipe
    aciona o atalho do overlay e depois o atalho/combo específico da Central
    de Scout.
  - **Caminho:** Abre o painel de Scout → seleciona "Contratar Olheiro" →
    escolhe especialização Tático, nível Experiente (custo exibido antes de
    confirmar) → confirma, o orçamento do clube é debitado (`transferbudget`)
    → cria uma nova missão: filtro por perfil "meia ofensivo com atributos de
    defesa" (fit fora de posição para volante), região = América do Sul +
    Europa, busca "completa" (não rápida) → confirma a missão.
  - **Clímax:** Alguns dias de carreira depois (avanço de `GJUr.currdate`),
    Felipe reabre o painel e vê a missão como "Concluída": um relatório com
    3-5 jogadores, radar de atributos por jogador, valores já bem precisos
    (poucas faixas abertas) por ser olheiro Experiente + busca completa.
  - **Resolução:** Felipe anota o nome do jogador preferido e vai até a tela
    nativa de transferências do FIFA para negociar manualmente.
  - **Caso de borda:** se o orçamento não cobrir o custo do olheiro/missão
    escolhida, a confirmação é bloqueada com mensagem clara do valor faltante.

- **UJ-2. Felipe usa a busca "rápida e suja" para uma visão geral do mercado.**
  Felipe, no início de uma nova carreira com pouco dinheiro, contrata um
  olheiro Generalista Júnior (barato) e dispara uma missão rápida sem filtro
  regional restrito, aceitando relatórios rasos (faixas largas, poucos
  atributos) só para ter uma visão panorâmica de onde há mais talento
  disponível antes de investir em olheiros melhores.

- **UJ-3. Felipe compara visualmente um achado do Scout com seu titular atual.**
  A partir de um relatório pronto, Felipe seleciona "comparar com jogador do
  elenco", escolhe seu volante titular, e vê dois radares de atributos
  sobrepostos lado a lado — decidindo se vale a pena substituir o titular.

## 3. Glossário

- **Olheiro (Scout)** — Entidade contratável pelo usuário, com uma
  **Especialização** e um **Tier**. Só existe dentro do Companion; não
  corresponde a nenhum registro em `career_scouts` (`zlrC`) do save.
- **Especialização** — Categoria de foco do Olheiro: `Caçador de Jovens`,
  `Caçador de Medalhões`, `Tático`, ou `Generalista`. Determina que tipo de
  **Missão** o olheiro executa com melhor precisão.
- **Tier** — Nível de habilidade do Olheiro: `Júnior`, `Experiente`, ou
  `Elite`. Afeta custo de contratação, velocidade de missão, e a
  **Qualidade** do **Relatório** resultante.
- **Missão de Scouting** — Uma pesquisa encomendada a um Olheiro contratado,
  com um conjunto de **Filtros** e um **Modo de Busca** (`Rápida` ou
  `Completa`). Tem um custo próprio e um tempo de conclusão medido em
  avanço de `GJUr.currdate`.
- **Modo de Busca** — `Rápida` (mais nomes, Qualidade menor, conclui mais
  rápido) ou `Completa` (menos nomes, Qualidade maior, conclui mais devagar).
- **Filtro** — Critério aplicado a uma Missão. Categorias: geográfico (país /
  liga / continente, multi-seleção), por atributo (ex: "mais driblador"),
  por Overall/Potencial, por **Fit Posicional**, ou por **Jogador de
  Referência** (jogador do elenco usado como base de similaridade).
- **Fit Posicional** — Grau de encaixe de um jogador em uma posição diferente
  da sua `preferredposition1` no save, calculado a partir do perfil de
  atributos (ex: um meia ofensivo com atributos de defesa altos tem Fit
  Posicional alto para volante).
- **Jogador de Referência** — Um jogador do elenco atual do usuário, usado
  como âncora de similaridade de perfil numa Missão ("encontre jogadores
  parecidos com X").
- **Relatório de Scouting** — Resultado de uma Missão concluída: lista de
  jogadores encontrados, cada um com dados parciais ou completos conforme a
  **Qualidade**.
- **Qualidade (do Relatório)** — Tridimensional: (1) precisão numérica dos
  atributos (faixa larga vs. valor quase exato), (2) quantidade de atributos
  revelados, (3) quantidade de jogadores retornados. Determinada pela
  combinação de Tier do Olheiro + Especialização (aderência ao tipo de
  Missão) + Modo de Busca.
- **Sonar de Cobertura** — Visualização em mapa mostrando quais
  países/ligas/continentes estão atualmente cobertos por Missões ativas ou
  concluídas.
- **Radar de Atributos** — Visualização em gráfico-aranha comparando os
  atributos de um jogador (do Relatório) — opcionalmente sobreposto ao de um
  Jogador de Referência.
- **Orçamento de Scouting** — [ASSUMPTION] Mesmo saldo que `transferbudget`
  no save (`dqXv`); não é uma moeda separada. Contratar Olheiros e Missões
  debita diretamente esse campo.

## 4. Features

### 4.1 Central de Scout — Painel e Acesso

**Descrição:** Um novo painel dentro do overlay ImGui já existente
(`fifa_overlay`), acessível por um atalho/combo dedicado enquanto o FIFA 16
está em Modo Carreira. Não depende de detecção automática de tela (M1
permanece pausado) — funciona como um painel "sempre disponível" quando uma
carreira está carregada. Realiza UJ-1, UJ-2, UJ-3.

**Requisitos Funcionais:**

#### FR-1: Abrir/fechar o painel de Scout via atalho dedicado

O usuário pode abrir e fechar o painel da Central de Scout através de um
atalho de teclado ou combo de controle configurável, distinto do atalho que
abre/fecha o overlay em si.

**Consequências (testáveis):**
- O atalho funciona com o FIFA 16 em fullscreen exclusivo, sem alternar modo
  de vídeo (reaproveita o hook de `Present()` já existente).
- Se nenhuma carreira estiver carregada no save ativo, o painel exibe um
  estado vazio explicando a limitação, em vez de falhar silenciosamente.

**Fora de Escopo:**
- Detecção automática de qual tela do menu do FIFA está aberta (ver M1 em
  `PROJECT_MEMORY.md`) — o painel abre independentemente da tela do jogo.

---

### 4.2 Contratação de Olheiros

**Descrição:** O usuário pode contratar Olheiros escolhendo Especialização e
Tier, com custo debitado do Orçamento de Scouting. Realiza UJ-1, UJ-2.

**Requisitos Funcionais:**

#### FR-2: Listar Olheiros disponíveis para contratação

O usuário pode ver uma lista de Olheiros disponíveis para contratar,
combinando as 4 Especializações (`Caçador de Jovens`, `Caçador de
Medalhões`, `Tático`, `Generalista`) com os 3 Tiers (`Júnior`, `Experiente`,
`Elite`) — até 12 combinações, cada uma com custo de contratação exibido.

**Consequências (testáveis):**
- Cada combinação Especialização × Tier tem um custo de contratação
  distinto e visível antes da confirmação.
- [ASSUMPTION] Não há limite de slots simultâneos de Olheiros contratados no
  v1 — simplicidade sobre realismo, revisitável em v2.

#### FR-3: Contratar um Olheiro debitando o Orçamento de Scouting

O usuário pode confirmar a contratação de um Olheiro, o que debita seu custo
do Orçamento de Scouting (`dqXv.transferbudget` no save ativo), usando a
técnica de escrita em memória já validada (sessão 4 do
`PROJECT_MEMORY.md`).

**Consequências (testáveis):**
- Se o Orçamento de Scouting disponível for menor que o custo do Olheiro, a
  contratação é bloqueada e a UI mostra o valor faltante.
- Após confirmação, o novo valor de `transferbudget` é lido de volta e
  exibido no painel (mesma técnica de re-leitura documentada na sessão 4).
- A escrita ocorre apenas quando o usuário confirma explicitamente — nunca
  automática ou implícita.

**Fora de Escopo:**
- Demitir/vender um Olheiro contratado de volta por reembolso parcial — v1
  não implementa isso (contratação é permanente até fim de sessão do
  Companion, ver §5).

---

### 4.3 Missões de Scouting e Filtros

**Descrição:** O núcleo de valor: o usuário encomenda uma Missão a um
Olheiro contratado, definindo Filtros e Modo de Busca. Realiza UJ-1, UJ-2.

**Requisitos Funcionais:**

#### FR-4: Criar uma Missão de Scouting com filtros combináveis

O usuário pode criar uma Missão vinculada a um Olheiro contratado,
combinando livremente: Filtro geográfico (país/liga/continente, seleção
múltipla), Filtro por Overall/Potencial (faixas mínimas/máximas), Filtro por
atributo dominante (ex: "mais driblador", "mais defensor" — baseado nos
atributos numéricos já lidos via `CZUM`), Filtro por Fit Posicional (ver
FR-6), e/ou Filtro por Jogador de Referência (ver FR-7).

**Consequências (testáveis):**
- Todos os filtros são combináveis na mesma Missão (não mutuamente
  exclusivos).
- Quanto mais amplo o recorte geográfico (ex: "todos os continentes"), menor
  a precisão/Qualidade aplicável ao Relatório resultante — refletido
  visualmente antes de confirmar a Missão (ex: um indicador de "precisão
  estimada").
- [ASSUMPTION] A Missão exige pelo menos um Olheiro contratado e disponível
  (não ocupado em outra Missão) — cada Olheiro só pode executar uma Missão
  por vez.
- [ASSUMPTION] A fórmula exata de cálculo de Qualidade (como Tier ×
  Especialização × Modo de Busca × amplitude geográfica combinam-se num
  valor final de precisão/atributos-revelados/quantidade-de-jogadores) não
  é definida neste PRD — fica para a fase de arquitetura/balanceamento,
  mesmo tratamento dado à fórmula de Fit Posicional (FR-6). Ver Questões
  em Aberto §8.

#### FR-5: Escolher Modo de Busca (Rápida vs. Completa)

O usuário pode escolher entre Modo de Busca `Rápida` (mais jogadores
retornados, Qualidade menor, conclusão mais rápida) e `Completa` (menos
jogadores, Qualidade maior, conclusão mais lenta), ao criar a Missão.

**Consequências (testáveis):**
- O tempo estimado de conclusão (em dias/semanas de carreira) é exibido
  antes da confirmação, calculado a partir de Tier do Olheiro + Modo de
  Busca + amplitude do Filtro geográfico.
- Trocar de Modo de Busca recalcula o custo da Missão exibido antes de
  confirmar.

#### FR-6: Filtrar por Fit Posicional (fora de posição)

O usuário pode incluir na Missão um Filtro de Fit Posicional: buscar
jogadores cujo perfil de atributos indica um bom encaixe em uma posição
diferente da sua `preferredposition1` registrada no save (ex: um meia
ofensivo com atributos de defesa altos aparecendo como candidato a volante).

**Consequências (testáveis):**
- O Relatório resultante indica explicitamente a posição nativa do jogador
  e a posição-alvo do Fit, com um indicador de força do encaixe.
- [ASSUMPTION] O cálculo de Fit Posicional usa uma fórmula de similaridade
  entre o perfil de atributos do jogador e um "perfil ideal" de referência
  por posição — a fórmula exata fica definida na arquitetura, não neste PRD.
- Esse Filtro tem maior aderência quando a Missão usa Olheiro de
  Especialização `Tático`.

#### FR-7: Filtrar por Jogador de Referência (perfil semelhante)

O usuário pode selecionar um jogador do elenco atual como Jogador de
Referência, para que a Missão busque jogadores de perfil de atributos
semelhante (ao invés de, ou combinado com, filtros absolutos).

**Consequências (testáveis):**
- A lista de jogadores do elenco disponível para seleção é lida diretamente
  do save (via as tabelas já mapeadas, ex: `RrqT`/`CZUM`).
- O Relatório indica, para cada jogador encontrado, um percentual/indicador
  de similaridade com o Jogador de Referência.
- [ASSUMPTION] A fórmula exata de similaridade de perfil (como os atributos
  do jogador encontrado são comparados aos do Jogador de Referência para
  gerar o percentual/indicador) não é definida neste PRD — fica para a fase
  de arquitetura, mesmo tratamento dado à fórmula de Fit Posicional (FR-6).
  Ver Questões em Aberto §8.

#### FR-8: Acompanhar progresso e conclusão de Missões ativas

O usuário pode ver, no painel, todas as Missões ativas com seu progresso
(baseado no avanço de `GJUr.currdate` desde a criação da Missão até o prazo
estimado) e recebe indicação clara quando uma Missão é concluída.

**Consequências (testáveis):**
- O progresso é recalculado a cada vez que o painel é aberto (não precisa de
  polling em tempo real) — comparando a data atual da carreira com a data de
  criação + duração estimada da Missão.
- Uma Missão concluída permanece visível/acessível até o usuário
  explicitamente arquivar ou descartar o Relatório.

**Notas:** *(questão em aberto específica desta feature)* Como o Companion
persiste o estado de Missões ativas entre sessões (o overlay é injetado e
ejetado por sessão de jogo) — precisa de um arquivo de estado próprio do
Companion, associado ao save ativo (decisão já tomada de não usar as
tabelas nativas `zlrC`/`apoo`, ver §5). Formato e localização exatos do
arquivo de estado ficam para a arquitetura. Ver Questões em Aberto §8.

---

### 4.4 Relatórios e Visualizações

**Descrição:** Apresentação dos resultados de uma Missão concluída, com as
duas visualizações "sonar" definidas na elicitação. Realiza UJ-1, UJ-3.

**Requisitos Funcionais:**

#### FR-9: Exibir Relatório de Scouting com Qualidade variável

O usuário pode abrir o Relatório de uma Missão concluída, vendo para cada
jogador encontrado: nome, idade, posição nativa (+ Fit Posicional se
aplicável), e atributos — com precisão, quantidade de atributos revelados, e
quantidade total de jogadores determinados pela Qualidade calculada da
Missão (Tier do Olheiro × Especialização × Modo de Busca × amplitude
geográfica).

**Consequências (testáveis):**
- Relatórios de baixa Qualidade mostram faixas (ex: "Overall: 70-80") em vez
  de valores exatos; Relatórios de alta Qualidade mostram valores exatos ou
  quase exatos.
- Relatórios de baixa Qualidade revelam um subconjunto de atributos (ex:
  overall, potencial, posição); Relatórios de alta Qualidade revelam o
  perfil completo (todos os atributos numéricos + work rate + pé
  preferido).

#### FR-10: Visualizar Radar de Atributos por jogador

O usuário pode visualizar, para qualquer jogador de um Relatório, um gráfico
radar (aranha) dos seus atributos revelados, com opção de sobrepor o radar
de um Jogador de Referência do elenco para comparação direta lado a lado
(realiza UJ-3).

**Consequências (testáveis):**
- Se a Qualidade do Relatório não revelou todos os atributos, o radar exibe
  apenas os eixos disponíveis (não inventa valores para os que faltam).
- A sobreposição com Jogador de Referência usa cores/contornos distintos e
  claramente legíveis.

#### FR-11: Visualizar Sonar de Cobertura

O usuário pode visualizar um mapa mostrando quais países/ligas/continentes
estão atualmente cobertos por Missões ativas e concluídas, dando uma visão
geral de onde a rede de scouting do usuário está atuando.

**Consequências (testáveis):**
- Regiões com Missão ativa são visualmente distintas de regiões com Missão
  concluída (Relatório disponível) e regiões nunca escaneadas.
- [ASSUMPTION] O nível de granularidade visual é por país (não por
  cidade/clube) no v1 — suficiente para orientar decisões sem exigir dados
  geográficos mais finos do que os já existentes no save (`Crbb.nationid`).

**Fora de Escopo:**
- Nível de "conhecimento" acumulado por região ao longo de múltiplas
  Missões (mecânica de FM onde a precisão de uma região melhora com
  histórico) — v1 trata cada Missão de forma independente. Ver
  Não-Objetivos §5.

## 5. Não-Objetivos (Explícitos)

- **A Central de Scout não escreve, cria, nem contrata jogadores de verdade
  no save.** Ela não efetiva transferências nem altera o elenco do usuário —
  o usuário sempre finaliza a ação manualmente na tela nativa do FIFA. O
  único campo do save escrito por esta feature é `dqXv.transferbudget`.
  **Emenda (2026-10-06, Felipe):** a sincronização com o scout nativo
  (Épico 7) também escreve na memória do jogo a lista de escolhidos nativa e
  o nível de conhecimento do jogador, só pelo `save_repo`, com interruptor
  nas configurações (ligado por padrão), conferência do valor antigo e sem
  nunca rebaixar o nível. Atributos de jogador, `zlrC`/`apoo` e transferências
  continuam fora.
- **Não há edição/escrita de atributos de jogador via Scout.** A limitação
  documentada em `PROJECT_MEMORY.md` (escrita per-player não resolvida)
  permanece — o Scout é 100% consumidor de dados já lidos, nunca escritor de
  atributos.
- **Não há "núcleo de olheiros detalhistas" (segunda etapa de
  aprofundamento) no v1.** Ideia validada e valiosa, mas adiada — ver §6.2.
- **Não há acúmulo de "conhecimento" progressivo por região entre Missões**
  (ao estilo Football Manager) no v1 — cada Missão calcula sua Qualidade de
  forma independente, sem memória de Missões anteriores na mesma região.
- **A Central de Scout não substitui nem se integra com `career_scouts`
  (`zlrC`)/`career_scoutmission` (`apoo`) nativos do save** — é um sistema
  paralelo e desacoplado. Decisão deliberada: só validamos escrita segura
  para um campo simples e isolado (`transferbudget`, int32 — ver sessão 4 do
  `PROJECT_MEMORY.md`); escrever uma estrutura nova e relacional (múltiplos
  registros de Olheiros/Missões) nessas tabelas exigiria replicar o mesmo
  trabalho de layout binário/checksum que se mostrou arriscado e
  malsucedido no projeto (ver "Escrita no save — BLOQUEADA por checksum" em
  `PROJECT_MEMORY.md`). Preferimos não arriscar corromper o save real do
  usuário por uma feature que pode viver inteiramente fora dele.
- **Não há suporte a outras versões do FIFA/EA FC** além da build atual do
  FIFA 16 (`16.0.2904053`) mapeada no projeto.
- **Não há multiplayer/Ultimate Team/Pro Clubs** — escopo é só Modo Carreira
  single-player.
- **Não há persistência de estado de Scout de forma "oficial"/sincronizada
  com múltiplos saves simultâneos** — o estado do Scout é atrelado ao save
  ativo identificado no momento (ver `identify_saves()` em
  `fifa16_search.py`).

## 6. Escopo do MVP (v1)

### 6.1 Em Escopo

- Painel de Scout dentro do overlay Rust/DLL existente, com atalho dedicado.
- Contratação de Olheiros: 4 Especializações × 3 Tiers, custo debitado do
  `transferbudget` real.
- Criação de Missões com filtros combináveis: geográfico (multi-seleção),
  Overall/Potencial, atributo dominante, Fit Posicional, Jogador de
  Referência.
- Modo de Busca Rápida vs. Completa, afetando custo, tempo e Qualidade.
- Progresso de Missão vinculado ao avanço de `GJUr.currdate`.
- Relatório de Scouting com Qualidade variável (precisão numérica,
  atributos revelados, quantidade de jogadores).
- Radar de Atributos por jogador, com sobreposição a Jogador de Referência.
- Sonar de Cobertura (mapa por país/liga/continente). **Removido em 2026-10-08** (commit `1c6d794`); a quinta aba virou a Base do Scout. Ver `epics.md`, Épico 4.

### 6.2 Fora de Escopo para o MVP

- **Núcleo de olheiros detalhistas** (2ª etapa de aprofundamento de um
  jogador já encontrado) — adiado para v2. **Entregue em 2026-10-08 como
  "Aprofundar agora" (Story 6.2)**: em vez de um novo tipo de Olheiro, o
  Generalista designado se dedica, mediante pagamento, a um Escolhido. `[NOTE FOR PM]`: essa ideia foi
  claramente valorizada na elicitação inicial e é um candidato forte para a
  primeira expansão pós-validação do v1.
- Demissão/venda de Olheiros contratados de volta. **Demissão implementada em 2026-10-08** ("Demitir", com confirmação, bloqueada durante uma Missão); a venda segue fora de escopo.
- Limite de slots simultâneos de Olheiros.
- Acúmulo de "conhecimento" progressivo por região entre Missões.
- Granularidade geográfica abaixo de país (cidade/clube) no Sonar de
  Cobertura.
- Qualquer forma de escrita de atributos de jogador ou efetivação de
  transferência dentro do save.
- Suporte a builds do FIFA 16 diferentes da atual, ou a outras versões do
  FIFA/EA FC.

## 7. Métricas de Sucesso

**Primária**
- **SM-1**: Felipe usa a Central de Scout de forma contínua nas suas
  próprias carreiras do FIFA 16 (pelo menos uma Missão criada por sessão de
  jogo relevante), preferindo-a à busca nativa de transferências. Valida
  FR-2 a FR-11.

**Secundária**
- **SM-2**: Pelo menos uma contratação real (fora do Companion, na tela
  nativa do FIFA) resulta diretamente de um jogador indicado por um
  Relatório de Scouting, dentro do primeiro mês de uso. Valida FR-9, FR-10.

**Contra-métricas (não otimizar)**
- **SM-C1**: Não otimizar para "número de Missões criadas" como métrica
  isolada de sucesso — criar Missões demais sem elas se traduzirem em
  decisões reais de elenco seria sinal de fricção/curiosidade vazia, não de
  valor genuíno. Contrabalança SM-1.

Esta é uma feature hobby validada por uso pessoal, não por métricas de
produto formais — o critério real e suficiente é: "eu volto a usar isso na
minha própria carreira, em vez de abandonar depois de testar uma vez."

## 8. Questões em Aberto

1. Qual o formato e a localização exatos do arquivo de estado próprio do
   Companion (Olheiros contratados, Missões, Relatórios), associado ao save
   ativo? A decisão de **não** usar as tabelas nativas `zlrC`/`apoo` já foi
   tomada (ver §5) — o que resta é o design concreto do arquivo, não a
   escolha do local de armazenamento.
2. Qual a fórmula exata de cálculo de Fit Posicional (perfil de atributos →
   score de encaixe por posição-alvo)? Fica para a fase de arquitetura/
   design técnico, não este PRD.
3. Qual a fórmula exata de cálculo de Qualidade (como Tier × Especialização
   × Modo de Busca × amplitude geográfica combinam-se em precisão numérica
   + atributos revelados + quantidade de jogadores) e de similaridade do
   Jogador de Referência? Assim como o Fit Posicional, ficam para a fase de
   arquitetura/balanceamento.
4. Qual a fórmula exata de custo de contratação e de Missão por combinação
   Especialização × Tier × Modo de Busca × amplitude geográfica? Precisa de
   uma tabela de balanceamento — candidato a um documento de design
   complementar (addendum ou arquitetura).
5. O que acontece se o usuário trocar de save ativo enquanto há Missões em
   andamento vinculadas ao save anterior? [Levantado mas não resolvido nesta
   sessão.]
6. Existe algum risco de o próprio ato de ler continuamente `CZUM`/`RrqT`
   via memória in-process, em paralelo a uma sessão longa de scouting,
   impactar performance do jogo (FPS) de forma perceptível? A sessão 5 do
   `PROJECT_MEMORY.md` já demonstrou soluções (thread separada, ~17s de
   scan) mas não foi testada em uso prolongado.

## 9. Índice de Suposições

- [§3, Glossário — Orçamento de Scouting] Não é uma moeda separada; é o
  mesmo `transferbudget` do save.
- [§4.2, FR-2] Não há limite de slots simultâneos de Olheiros contratados no
  v1.
- [§4.3, FR-4] Cada Olheiro só pode executar uma Missão por vez.
- [§4.3, FR-4] A fórmula exata de cálculo de Qualidade (Tier × Especialização
  × Modo de Busca × amplitude geográfica) não é definida neste PRD, fica
  para arquitetura/balanceamento.
- [§4.3, FR-6] O cálculo de Fit Posicional usa uma fórmula de similaridade
  de perfil de atributos vs. perfil ideal por posição — fórmula exata não
  definida neste PRD, fica para arquitetura.
- [§4.3, FR-7] A fórmula exata de similaridade de perfil com o Jogador de
  Referência não é definida neste PRD, fica para arquitetura.
- [§4.4, FR-11] Granularidade do Sonar de Cobertura é por país no v1, não
  por cidade/clube.
