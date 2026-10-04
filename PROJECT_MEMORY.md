# FIFA 16 Companion — Memória do Projeto

> Leia este arquivo primeiro em qualquer nova sessão. Ele resume o
> objetivo, o que já existe, decisões tomadas (e por quê), bugs
> corrigidos, limitações conhecidas, e os próximos passos sugeridos.
> Detalhes técnicos extensos da investigação de memória (descartada)
> estão em `fifa_process_identifier/MEMORY_INVESTIGATION_NOTES.md`.

## Objetivo do projeto

Criar um "companion app" para o FIFA 16 (modo carreira) que:
1. Lê diretamente o save do jogo (sem intermediar pelo próprio FIFA).
2. Oferece uma interface própria, mais rica, para tarefas específicas
   — o primeiro caso de uso implementado é **busca avançada de
   jogadores** (o equivalente à tela de pesquisa de transferências,
   mas com filtros melhores e fora do jogo).
3. Abre/fecha via atalho (teclado ou controle), para uso enquanto o
   FIFA está rodando.

Ideia original mais ambiciosa (documentada mas não implementada):
detectar automaticamente qual menu do FIFA está aberto (via leitura
de memória do processo) e abrir a interface certa sozinha. Essa parte
foi **pausada** por complexidade (ver seção "M1 — descartado" abaixo)
e substituída por um atalho manual (MVP pragmático).

## Estado atual: MVP funcional

O fluxo abaixo já funciona de ponta a ponta, validado com o save real
do usuário:

```
Usuário pressiona Ctrl+Shift+P (teclado)
  ou combo do controle (LEFT_THUMB + START)
        │
        ▼
Electron mostra/esconde a janela do Companion
        │
        ▼
Interface de busca (renderer/index.html)
        │  filtros: nome, posição, idade, overall,
        │  potencial, nacionalidade, pé preferido
        ▼
main.js chama `python fifa16_search.py <flags>` (subprocess)
        │
        ▼
fifa16_search.py:
  - localiza o save mais recente automaticamente
  - lê fifa_ng_db.db (banco estático: nomes, nações)
  - lê DATA do save (banco dinâmico: atributos dos jogadores)
  - decodifica via fifa16_db_parser.py
  - aplica filtros, retorna JSON
        │
        ▼
Electron recebe JSON, popula a tabela de resultados
```

### Estrutura de arquivos

```
FIFA_EDITOR/
├── PROJECT_MEMORY.md              <- este arquivo
├── fifa16_db_parser.py            <- parser de baixo nível (t3db v8)
├── fifa16_search.py                <- motor de busca (usa o parser)
├── fifa16_search_config.json       <- gerado em runtime: último save usado
│
├── companion/                      <- app Electron (interface)
│   ├── main.js                     <- processo principal, hotkey, IPC, subprocess
│   ├── preload.js                  <- ponte contextBridge (segurança)
│   ├── package.json
│   ├── start_companion.ps1         <- inicia Electron + gamepad watcher juntos
│   └── renderer/
│       ├── index.html              <- formulário de filtros + tabela
│       ├── renderer.js             <- lógica do front-end
│       └── style.css
│
└── fifa_process_identifier/        <- ferramentas Python de baixo nível
    ├── gamepad_watcher.py           <- monitor XInput (combo do controle)
    ├── gamepad_config.json          <- combo configurado + índice do controle
    ├── process.py                   <- find/open processo do FIFA (Windows API)
    ├── modules.py                   <- lista módulos carregados (EnumProcessModulesEx)
    ├── memory.py                    <- snapshot/diff de memória (usado na investigação M1)
    ├── pointer_scan.py, pointer_bfs*.py, stable_scan.py, analyze.py,
    │   find_hub.py, find_string.py, watch_pointer.py, main.py
    │                                 <- ferramental da investigação M1 (ver notas)
    └── MEMORY_INVESTIGATION_NOTES.md <- detalhes técnicos completos do M1
```

### Como rodar

```powershell
# Opção 1: tudo junto (Electron + gamepad watcher)
powershell -ExecutionPolicy Bypass -File companion\start_companion.ps1

# Opção 2: individualmente (útil para debug)
cd companion
$env:Path += ";C:\Program Files\nodejs"   # ver "Pegadinhas de ambiente" abaixo
$env:SHOW_ON_STARTUP = "1"                # força janela visível ao iniciar (debug)
npm start

python fifa_process_identifier\gamepad_watcher.py               # watcher normal
python fifa_process_identifier\gamepad_watcher.py --calibrate   # descobrir botões do controle

# CLI direto do motor de busca (sem Electron)
python fifa16_search.py --position ST --min-potential 85 --limit 10
python fifa16_search.py --list-saves     # lista saves disponíveis
```

## Decisões importantes e por quê

### 1. Por que Electron + Python (híbrido) em vez de só uma linguagem?

- O parser binário do save (`fifa16_db_parser.py`) já existia e
  funciona (bit-packing customizado, árvores Huffman para strings).
  Reescrever isso em JS seria retrabalho arriscado.
- Electron dá `globalShortcut` (hotkey de teclado que funciona mesmo
  sem foco) de graça, e uma interface HTML/CSS é mais rápida de
  iterar visualmente que Tkinter.
- Comunicação: `main.js` chama `fifa16_search.py` via
  `child_process.spawn` e faz `JSON.parse` do stdout. Simples,
  sem servidor HTTP rodando em background.

### 2. Por que não há overlay real sobre o FIFA?

O FIFA 16 do usuário roda em **fullscreen exclusivo do DirectX**
(confirmado: Alt+Tab causa troca de modo de vídeo). Nesse modo, **não
existe overlay de janela externa sem hook da swap chain via DLL
injection** no processo do jogo — é a mesma técnica usada por Discord
Overlay/Steam Overlay/RTSS, e é um projeto grande por si só (C++/Rust,
MinHook, hook de `Present()`). Decisão: aceitar que abrir o Companion
tira o FIFA da tela cheia temporariamente. Ver "Próximos passos" para
retomar isso no futuro se fizer sentido.

### 3. Por que o atalho de controle usa LEFT_THUMB+START e não outro combo?

Testamos nesta ordem:
- **Paddles traseiros do 8BitDo do usuário**: não emitem NENHUM sinal
  XInput (nem em bits conhecidos, nem desconhecidos). Provavelmente
  precisam ser remapeados via app/firmware do fabricante para emular
  outro botão. Não resolvido.
- **LEFT_THUMB + RIGHT_THUMB (L3+R3)**: detectável via XInput, mas
  **conflita com um atalho nativo do próprio FIFA** (o jogo reage ao
  mesmo combo — saiu do que estava fazendo). Descartado.
- **LEFT_THUMB + START**: funciona sem conflito aparente com o FIFA.
  Configurado como padrão em `gamepad_config.json`. Tem uma limitação
  de timing (ver item 4 abaixo).

Para descobrir/testar outros combos: `python gamepad_watcher.py --calibrate`
mostra em tempo real quais bits `wButtons` acendem para cada botão
pressionado (incluindo bits "desconhecidos" fora do enum padrão,
caso algo inesperado apareça).

### 4. Por que às vezes preciso apertar o atalho duas vezes?

Ao sair do fullscreen exclusivo, a transição de modo de vídeo do
Windows não é instantânea. `mainWindow.show()` é chamado
imediatamente, mas o efeito visual pode demorar um instante para
aparecer — e se o usuário apertar o atalho de novo rapidamente nesse
meio tempo, o Electron já considera a janela "visível" (mesmo sem o
efeito ter completado) e a esconde de novo. **Não resolvido.**
Aceito como limitação conhecida do MVP. Ideia para o futuro: detectar
a transição de fullscreen de alguma forma, ou adicionar um pequeno
delay/retry ao `show()`.

## Bugs de decodificação corrigidos (importante para qualquer trabalho futuro nos dados)

Todos validados cruzando com jogadores reais conhecidos no save do
usuário (Mbappé, Haaland).

| Campo | Bug | Causa raiz | Correção |
|---|---|---|---|
| Nomes com acento (stdout) | "Mbappé" virava "Mbapp?" ao passar por subprocess | Console/pipe do Windows usa codepage não-UTF8 por padrão | Força `sys.stdout`/`stderr` para UTF-8 explicitamente em `fifa16_search.py` |
| `nationality` | Off-by-one — Mbappé (França) resolvia para "FYR Macedonia" | `fifa16_db_parser.load_metadata` indexava campos do XML só por `shortname` do campo (ex: "LEtt"), mas o mesmo shortname é reusado em tabelas diferentes com `rangelow` diferentes | Metadados agora indexados por `(tabela, campo)`, não só `campo`. Ver `load_metadata()` retornando 3 valores agora (`tables, fields_global, fields_by_table`) |
| `age` (idade) | Jogadores "regen" apareciam com 9-14 anos mesmo sendo profissionais adultos na carreira | Idade calculada usando `date.today()` (data real do PC), mas a carreira do save está ambientada em **2035** (calendário interno do jogo, não a data real) | Lê `currdate` da tabela `GJUr` (formato `YYYYMMDD` puro, ex: `20351102`) e usa como referência em vez da data do sistema |
| `preferredfoot` | Filtro "left" retornava 0 resultados sempre | Assumi valores 0/1, mas os valores reais no save são **1 e 2** | Corrigido: `1 = Right, 2 = Left` (confirmado por metadata XML `rangelow=1/rangehigh=2` E validado com Mbappé=1/Right, Haaland=2/Left — Haaland é canhoto na vida real) |
| `birthdate` | Formato desconhecido inicialmente | — | É um inteiro de 20 bits = dias desde a época `1582-10-14` (dia seguinte à reforma do calendário Gregoriano). Validado comparando datas reais de Mbappé/Haaland com os valores brutos |

**Lição geral**: o metadata XML (`fifa_ng_db-meta.xml`) reusa
`shortname`s de campo entre tabelas diferentes com significados/ranges
distintos. Qualquer novo campo decodificado deve ser validado contra
pelo menos 1-2 jogadores reais conhecidos antes de confiar no
resultado — vários desses bugs só foram pegos por acaso ao notar
nomes/nações/idades absurdas na interface.

## Dados do save — referência rápida de tabelas conhecidas

Formato: banco de dados customizado t3db v8, comprimido, com strings
em árvore Huffman. Parser em `fifa16_db_parser.py`.

- **Banco estático** (`D:\Program Files\FIFA 16\data\db\fifa_ng_db.db`,
  não muda entre saves):
  - `BGwe`: `nameid -> name` (31287 nomes, primeiro/último nome de
    jogadores licenciados)
  - `Crbb`: `nationid -> nationname` (224 nações, `nationid` 0-223)
- **Save ativo** (`Documents\FIFA 16\0\FIFA16\<hash>\DATA`):
  - `CZUM`: tabela principal de jogadores (~32k registros, 105
    campos). Campos-chave: `playerid`, `firstnameid`, `lastnameid`,
    `overallrating`, `potential`, `preferredposition1` (enum 0-27,
    ver `POSITION_NAMES` em `fifa16_search.py`), `nationality`
    (referencia `Crbb.nationid`), `preferredfoot` (1=Right/2=Left),
    `birthdate`, `height`, `weight`
  - `GJUr`: metadados da carreira (1 registro). Campo `currdate`
    = data atual da carreira em formato `YYYYMMDD`
  - Muitas outras tabelas existem (times, táticas, scouts, staff,
    negociações etc.) mas não foram mapeadas ainda — ver
    `RrqT`, `lyxL`, `AGmV`, `mDGw`, `qdZF` como pontos de partida se
    for necessário (nomes de campo já vêm do metadata XML).

Localização do save: `Documents\FIFA 16\0\FIFA16\` contém várias
pastas com nome hash, cada uma com `DATA` + `INDEX`. A pasta correta é
a que tem o `DATA` mais recente por `mtime` E tamanho > 1MB (saves
"completos" ficam ~9.2MB; existe pelo menos uma pasta de ~370KB que
parece ser um save parcial/staging e deve ser ignorada — ver
`FifaDatabase.find_save_dirs()`).

## Pegadinhas de ambiente (Windows, PowerShell)

- **Node.js instalado após abrir o terminal**: o PATH da sessão atual
  não é atualizado automaticamente. Sintoma: `electron.cmd` reclama
  que não acha `node`. Solução: `$env:Path += ";C:\Program Files\nodejs"`
  no início da sessão (já embutido em `start_companion.ps1`).
- **Scripts `.ps1` bloqueados por política de execução**: rodar com
  `powershell -ExecutionPolicy Bypass -File script.ps1` em vez de
  mudar a política do sistema.
- **`npm` via alias PowerShell falha, `npm.cmd` funciona**: sempre
  invocar `& "C:\Program Files\nodejs\npm.cmd"` explicitamente em vez
  de `npm` quando rodando via ferramentas de automação.
- **Redirecionamento `>` do PowerShell grava em UTF-16 com BOM**, não
  UTF-8 — corrompe JSON com acentos se você fizer
  `python fifa16_search.py ... > out.json`. Prefira rodar via
  `subprocess.run(..., encoding="utf-8")` a partir de outro script
  Python, ou `Get-Content -Encoding UTF8` para ler de volta.
- **`fs.watch` do Node no Windows pode disparar o evento "change" mais
  de uma vez** para uma única escrita de arquivo — o sinal do gamepad
  usa `toggle:<timestamp_ns>` (não só `"toggle"`) para garantir que o
  conteúdo mude a cada ativação, e o `main.js` tem debounce de 250ms
  adicional.
- **Processos em background via `Start-Process` + redirecionamento**:
  o tool de shell usado nas sessões reporta "timeout"/"kill" mesmo
  quando o processo continua rodando normalmente em segundo plano —
  isso é comportamento esperado, não um erro real. Sempre confirmar
  com `Get-Process` depois.

## M1 — Detecção automática de menu via memória (PAUSADO)

Tentativa original: monitorar a memória do processo `fifa16.exe` para
detectar automaticamente qual tela/menu está ativo, e abrir a
interface certa sozinha (sem hotkey manual).

**Resultado**: não encontramos uma variável de memória estável e
confiável para "tela atual" depois de ~2h de investigação combinando:
- Diff bruto de memória entre snapshots (Python) — ruído demais
  (motor do jogo sempre altera muita memória em segundo plano).
- Toggle scan manual no Cheat Engine — convergiu de 6 milhões para
  1.561 candidatos, ainda inviável de inspecionar manualmente.
- Pointer scan multi-nível (BFS) em Python, partindo de strings como
  `TransferPlayerSearch`, `CareerHubViewModel` — achamos uma âncora
  estável (`fifa16.exe+0x3357378`) mas ela apontava para um buffer de
  log/asset genérico, não o estado de navegação real.

**Cuidado se retomar**: usar "Find out what accesses this address" do
Cheat Engine (breakpoint de hardware) **causou o fechamento abrupto do
FIFA**, possivelmente por conflito com o mod "FIFA Friends"/CG Server
que o usuário roda (proteção anti-tamper ou conflito de debug
registers). Evitar técnicas de debugging ativo com o mod rodando;
preferir leitura passiva (`ReadProcessMemory`) ou testar com o mod
desligado primeiro.

Detalhes completos, incluindo todo o ferramental construído
(`pointer_scan.py`, `pointer_bfs.py`, `stable_scan.py`, etc., todos
reutilizáveis) estão em
`fifa_process_identifier/MEMORY_INVESTIGATION_NOTES.md`.

## Exploração de novos recursos (sessão 2) — minifaces, stats, escrita

Sessão focada em avaliar viabilidade de expandir o MVP: acesso a
minifaces, estatísticas de jogadores (gols/assistências/forma/nota),
workrate, e — o ponto mais importante — **escrita no save**.

### Minifaces — VIÁVEL, confirmado funcionando

- Localização: `D:\Program Files\FIFA 16\data\ui\imgAssets\heads\p<PLAYERID>.dds`
  (ex: `p239085.dds` = Haaland). ~33.255 arquivos, um por jogador
  (incluindo genéricos extras).
- Formato: DDS (DirectDraw Surface), 128x128, RGBA.
- **Pillow lê DDS nativamente** (`Image.open(path)`), sem precisar de
  plugin extra. Testado e confirmado visualmente pelo usuário: a
  imagem convertida para PNG é de fato o rosto do jogador (Haaland).
- Também existem pastas irmãs relevantes: `data\ui\imgAssets\youthheads\`
  (jogadores jovens/academia), `data\sceneassets\faces\` e
  `data\sceneassets\heads\` (modelos 3D completos, não apenas a
  textura 2D da miniface — não explorados ainda).

### Estatísticas de jogadores — mapeado no save (read-only)

Tabelas confirmadas com dados reais (cruzado com Haaland, playerid
239085):

- **`TtHG` (career_playermatchratinghistory)**: histórico de nota por
  partida. Campos: `playerid`, `date` (YYYYMMDD), `rating` (nota do
  jogo, ex: 8, 6, 7...), `minsplayed`, `position`, `artificialkey`.
  Não há campo de "nota média" pronto — precisa ser calculado
  agregando essas linhas por jogador.
- **`RrqT` (teamplayerlinks)**: estado da temporada atual por
  jogador/time. Campos: `leaguegoals`, `leagueappearances` (nota:
  apareceu como 0 mesmo com Haaland tendo 16 partidas em `TtHG` —
  possível que só conte após algum evento, ou é outro tipo de
  "appearances" — não totalmente esclarecido), `form` (0-5, "forma"
  atual), `prevform`, `yellows`, `reds`, `istopscorer`,
  `isamongtopscorers`, `jerseynumber`.
- **`YMgA` (career_playasplayerhistory)**: goals, assists,
  appearances, matchratings, shotsontarget, passesontarget,
  tacklesontarget — só populada se o usuário já usou o modo "jogar
  como jogador"; estava vazia (0 rows) no save testado.
- **`DvsP` (career_playercontract)**: contrato atual — `wage`,
  `extension_years`, `contract_status`, datas de negociação.
- **`QWbR` (career_transferoffer)**: histórico/estado de propostas de
  transferência — `offeredfee`, `offeredwage`, `offeredbonus`,
  `desiredfee`, `valuation`, `isloan`, `result`, `stage`,
  `offerteamid`, `teamid`, `playerid`. Rica o suficiente para
  reconstruir uma tela de "criar proposta de transferência" (ainda
  como consulta/simulação, não efetivando no jogo).
- **`oOlF` (career_transferlist)**: lista de jogadores no mercado de
  transferências ativo (transfer list). Campos: `playerid`, `teamid`,
  `preferredposition1`, `overallrating`, `leagueid`, `potentialtype`,
  `pitcharea`.
- **`zlrC` (career_scouts)** / **`apoo` (career_scoutmission)**:
  estrutura de olheiros contratados e missões de scouting — estavam
  vazias no save testado (usuário não usa essa feature nesse save).

### Workrate — confirmado

`CZUM.attackingworkrate` / `CZUM.defensiveworkrate`, valores 0/1/2 =
Low/Medium/High. Validado: Haaland tem `attacking=2 (High)`,
`defensive=0 (Low)` — consistente com um atacante puro. **Isso já
estava mapeado antes desta sessão**; o usuário pediu para confirmar e
foi validado com sucesso (não precisa de mais trabalho de leitura).
O que falta é a capacidade de **escrever** esse campo (ver seção de
escrita abaixo).

### Escrita no save — BLOQUEADA por checksum, caminho alternativo identificado

**Tentativa 1: escrita direta no arquivo DATA.** Implementamos
`write_packed_int()` em `fifa16_db_parser.py` (espelha
`read_packed_int`, preserva bits vizinhos — validado com 2000 testes
aleatórios sem falha). Fizemos round-trip completo: altersamos
`overallrating` de um jogador, single-byte diff confirmado (só 1 byte
mudou em 9.2MB), re-parse do disco confirmou o novo valor. **Porém, ao
carregar no FIFA real, o save foi rejeitado como "corrompido"** — duas
vezes, em dois saves de teste diferentes (`717036e3` e `7a096416`,
ambos restaurados com sucesso a partir de backup).

**Causa raiz identificada** (via pesquisa do usuário + confirmação
factual): o save do FIFA usa um **CRC32 interno para validar
integridade**. Confirmamos:
- Existe uma string `"SaveType_Career\0"` no offset `0x74` do arquivo
  `DATA`.
- Logo em seguida, no offset `0x84`, há um campo de 4 bytes
  (`0xD6AB1DF9` no save testado) que é o candidato mais forte a ser o
  checksum armazenado — posição bate exatamente com a descrição
  técnica encontrada pelo usuário ("logo após a string de
  identificação, existe um campo de 4 bytes para o CRC32").
- **Não conseguimos reproduzir esse valor** testando >20 combinações
  de zona de cálculo (do início do arquivo, do fim da string, de
  0x92, até o fim do arquivo ou até o fim dos dados úteis antes do
  padding de zeros) × variantes (CRC32 padrão zlib, seed -1, Adler32,
  com/sem zerar o campo antes de calcular). O algoritmo exato usado
  pela EA não é o CRC32 "livro-texto" aplicado de forma óbvia a essas
  regiões, ou há alguma transformação adicional não identificada.
- Scripts de teste (`test_crc_hypothesis*.py`) foram removidos após
  uso — a lógica de teste está descrita aqui caso alguém queira
  refazer com mais tempo.

**Caminho alternativo identificado e recomendado: editar via
memória, não via arquivo.** Conforme confirmado pela pesquisa do
usuário e pelo padrão observado nos repositórios da comunidade
(xAranaktu mantém "Cheat Table"/"Live Editor" para cada versão do
FIFA/FC, de FIFA 19 a FC 26 — não há um especificamente para FIFA 16
no GitHub dele, mas o padrão de ferramenta é sempre o mesmo):

1. Encontrar o endereço de memória do campo a editar **enquanto o
   FIFA está rodando** (heap dinâmico, não o arquivo).
2. Escrever o novo valor direto na memória do processo
   (`WriteProcessMemory`).
3. Deixar o **próprio FIFA salvar** pelo menu do jogo — nesse
   momento o motor recalcula o checksum sozinho corretamente, porque
   é ele mesmo gravando o arquivo. Isso elimina completamente o
   problema do checksum desconhecido.

Essa abordagem reaproveita diretamente o ferramental construído no
M1 (`pointer_scan.py`, `pointer_bfs.py`, `memory.py`, `modules.py`).

**Cheat Table da comunidade já disponível**: o usuário possui
`FIFA 16 CT Version 7.1 Complete.CT` (Cheat Engine, formato XML),
copiado para `fifa_process_identifier/FIFA16_CT_v7.1.ct`. Parseado
com `fifa_process_identifier/parse_cheat_table.py` (script utilitário
mantido — reexecutar se precisar reprocessar o `.ct`), resultando em
`cheat_table_parsed.txt` com 78 endereços documentados.

Descobertas da Cheat Table:
- **Endereço-base principal**: `"fifa16.exe"+034D7908` (offset fixo
  dentro do módulo — deve sobreviver a ASLR entre execuções, igual ao
  padrão que já tínhamos validado no M1).
- Path de ponteiros típico: `[base] -> offset1 -> offset2 -> 0 ->
  E0 -> 120` — os dois últimos offsets (`E0`, `120`) se repetem em
  quase TODOS os atributos (Reactions, Dribbling, Ball Control,
  Balance, Agility, Sprint Speed, Acceleration, Shooting stats,
  Passing stats, Defending stats, Physical stats, GK stats) — isso é
  fortemente sugestivo de que `E0`/`120` levam a uma struct comum
  "atributos do jogador selecionado atualmente na UI", e o offset
  variável (`offset1 -> offset2`) é o que seleciona QUAL atributo
  dentro dessa struct.
- Outro endereço-base: `"fifa16.exe"+034D9258` (usado para `Wage` e
  `Transfer` — provavelmente uma struct diferente, relacionada a
  contrato/transferência do jogador selecionado).
- Um terceiro endereço: `"fifa16.exe"+034D7910` (usado só para
  `Player Position`).
- **NÃO documentados na CT do usuário** (precisam ser descobertos por
  extensão do padrão acima, ou novo pointer scan): `overallrating`,
  `potential`, `workrate` (attacking/defensive), `nameid`
  (firstname/lastname), `nationality`. O padrão comum de offsets
  (`E0`/`120`) é o melhor ponto de partida — provavelmente esses
  campos estão na mesma struct, só com um offset de campo diferente
  dentro dela (mesma técnica: fixar o jogador selecionado, variar o
  offset final testando valores próximos aos já conhecidos, tipo
  varredura ao redor de `0x120`).

### Identificação de saves — resolvido, útil e não relacionado a escrita

Durante os testes, tivemos dificuldade real de saber qual pasta de
save (`Documents\FIFA 16\0\FIFA16\<hash>\`) correspondia a qual save
visível na lista do FIFA. Resolvido: cada save tem uma string única
no formato `"<time1> pos <time2>"` (nome do próximo confronto)
embutida no arquivo `DATA` — é isso que o FIFA mostra como nome do
save na lista de "Continuar Carreira". Adicionado como
`FifaDatabase.identify_saves()` / `--identify-saves` no
`fifa16_search.py`, retornando manager (via tabela `mPrV`), data da
carreira (via `GJUr.currdate`) e data de modificação do arquivo — mas
**não inclui o nome do confronto ainda** (poderia ser adicionado
extraindo essa string via regex `rb"[A-Za-z][A-Za-z0-9 .]{2,20} pos [A-Za-z][A-Za-z0-9 .]{2,20}"`
do `DATA`, replicando a lógica usada ad-hoc durante a investigação).

### Problema real do usuário identificado: sobrenomes repetidos em saves avançados

O usuário reporta que, em saves muito avançados (carreira longa,
muitos jogadores "regen" gerados), o pool de sobrenomes genéricos se
esgota e o jogo passa a gerar excesso de jogadores com o mesmo
sobrenome (ex: "de Oliveira" — visível nos próprios exemplos de dados
usados nesta sessão). Ele gostaria de poder editar/reapontar os
`nameid` (via `firstnameid`/`lastnameid` em `CZUM`, que referenciam
`BGwe.nameid` no banco estático) para diversificar. Isso é
tecnicamente viável em leitura (já sabemos exatamente como o mapeamento
funciona — ver seção "Dados do save"), mas **depende de resolver
escrita primeiro** (via arquivo, se o checksum for decifrado, ou via
memória, seguindo o caminho recomendado acima).

## Exploração de escrita em memória (sessão 3) — objetivo: edição em lote para simulação de treinamento

Contexto: o usuário quer construir uma feature de **simulação estatística
de treinamento de jogadores** que precisa editar atributos de muitos
jogadores de uma vez (não um jogador por vez pela UI). Isso motivou
reavaliar tanto a escrita via memória (M1/sessão 2) quanto retomar a
escrita via arquivo (checksum).

### Cadeias de ponteiros da Cheat Table — confirmado: só funcionam com a tela certa aberta

Testado ao vivo com o FIFA rodando (`resolve_chain.py`, novo utilitário
criado nesta sessão, em `fifa_process_identifier/`): as cadeias de
`FIFA16_CT_v7.1.ct` (ex: `fifa16.exe+0x34D7908 -> 0x68 -> 0x38 -> 0 ->
0xE0 -> 0x120` para "Age") retornam ponteiro **nulo** quando a tela
ativa é o "Perfil do Jogador" (menu que mostra atributos mas não
permite editar). O usuário confirmou que essas cadeias da Cheat Table
só populam de fato na tela de **edição** de jogador (que não está
disponível no fluxo normal de Modo Carreira do FIFA 16 do usuário —
só encontrou o "Perfil"). Não foi possível validar as cadeias ao vivo
por falta de acesso a essa tela específica nesta sessão.

**Implicação**: mesmo que resolvêssemos essas cadeias, essa técnica é
inerentemente per-jogador com a tela certa aberta — **não serve** para
edição em lote de milhares de jogadores.

### Descoberta: `fifa16.exe` é protegido por packer/anti-tamper

Análise do PE header do executável (`find_crc_xref.py`) revelou que
quase todas as seções (`.data`, `.xpdata`, `.bss ta`, `.pdata a`,
`.tls em`, etc — nomes truncados/não-padrão, indício de ofuscação)
apontam para o mesmo `RawPtr=0x400` no arquivo em disco, com
`RawSize=0` ou desproporcional ao `VirtualSize` declarado. Isso é o
padrão clássico de packers/anti-tamper tipo Denuvo/VMProtect/Themida:
o código real só existe desempacotado em memória durante a execução,
nunca no arquivo em disco. Isso explica tanto por que não conseguimos
rastrear estaticamente quem usa a tabela CRC32 (`find_crc_table.py`
achou a tabela padrão em `0x3D4C0` do exe, mas não há xref válida no
arquivo) quanto por que o Cheat Engine "Find out what accesses this
address" (breakpoint de hardware) derrubou o jogo na investigação
anterior (M1) — típico de anti-tamper reagindo a debugging ativo.

### Checksum do arquivo DATA — confirmado como impraticável de decifrar estaticamente

Força bruta exaustiva rodada nesta sessão (`test_crc5.py`, `test_crc6.py`):
- Testado o campo de 4 bytes em offset `0x84` (logo após
  `"SaveType_Career\0"` em `0x74`) contra **todas as janelas possíveis**
  `(start, end)` nos primeiros 512-1024 bytes do arquivo, em **15 saves
  reais diferentes** do usuário simultaneamente (exigindo que a mesma
  janela batesse em todos os 15 — praticamente impossível por
  coincidência).
- Variantes testadas: CRC32 reflected padrão (zlib), seed 0 e
  0xFFFFFFFF; CRC32/BZIP2, CRC32/MPEG2, CRC32C (Castagnoli), todas via
  `crcmod`, non-reflected e reflected, com e sem XOR final.
- **Nenhuma combinação bateu.** Também confirmado via
  `find_crc_table.py` que a tabela de lookup CRC32 "reflected" padrão
  (polinômio 0xEDB88320) **existe** no executável (offset `0x3D4C0`),
  então o algoritmo é provavelmente CRC32 de alguma variante — mas o
  range exato e/ou uma transformação adicional não foram identificados,
  e não são rastreáveis estaticamente por causa do packer (seção acima).

**Pesquisa externa confirma o beco sem saída**: o usuário indicou
`sammygriffiths/fifa-career-save-parser` (parser JS, somente leitura,
cobre FIFA 17+), que credita `xAranaktu/FIFA-Tracker` como inspiração.
Investigação do portfólio de `xAranaktu` (mantenedor da Cheat
Table/Live Editor mais completa da comunidade, cobrindo FIFA 19 até
FC 26) mostra que a abordagem dele **nunca é escrita direta no
arquivo de save** — é sempre um "Live Editor": script Lua rodando
dentro de uma engine de Cheat Engine, hookando o jogo em execução,
editando memória diretamente, e deixando o **próprio jogo salvar**
(o que recalcula o checksum automaticamente, contornando o problema
por completo). O fato de ninguém na comunidade — nem quem faz isso
profissionalmente há ~6 anos, para toda versão do FIFA/FC — ter
decifrado o checksum é evidência forte de que a proteção é
proposital e robusta o suficiente para não valer a pena insistir
nesse caminho com engenharia reversa estática.

A API Lua documentada do FIFA-23-Live-Editor
(`lua/DOC.MD` no repo, lido nesta sessão) confirma o padrão: funções
como `GetDBTableRows`/`EditDBTableField`/`SetPlayerForm`/
`PlayerSetValueInDevelopementPlan` operam editando objetos de memória
ao vivo, exigindo apenas "estar em Career Mode" (não uma tela
específica) para a maioria — mas isso é porque o Live Editor já fez
o trabalho pesado de mapear ponteiros estáveis por versão do jogo,
trabalho que teríamos que refazer do zero para FIFA 16 (nenhum
projeto do xAranaktu cobre FIFA 16 especificamente).

### Nova descoberta: a database completa de jogadores (`CZUM`) existe como blob no heap, mas não é a fonte "viva"

Testando a hipótese de que o motor mantém as mesmas estruturas
binárias do save (`DB\x00\x08\x00\x00\x00\x00` + tabelas) residentes em
RAM durante toda a sessão de carreira (não só quando uma tela
específica está aberta):

- **Confirmado**: com o FIFA em Modo Carreira (qualquer tela),
  `find_db_in_memory.py` (novo script) encontra a assinatura de
  database em várias regiões PRIVATE+COMMIT do heap. Uma delas, no
  endereço `0x8BE70000+0xE7000` (varia por execução — heap não é
  estável entre sessões, precisa ser rebuscado toda vez), contém um
  banco **completo com 38 tabelas**, incluindo `CZUM` (a tabela
  principal de jogadores) com **32.602 registros** — o mesmo número
  de registros do save em disco.
- **Validado com dados reais**: parseando esse blob com
  `fifa16_db_parser.parse_database`/`decode_table` (sem nenhuma
  modificação), o registro do playerid 74449 (Ibrahim Mbaye) leu
  `acceleration=79, agility=80, jumping=47, strength=43` — batendo
  **exatamente** com os valores que o usuário via na tela do jogo
  naquele momento. A região tem proteção `PAGE_READWRITE` (0x4).
- **Teste de escrita (`test_memory_write.py`, novo script)**: usando
  `WriteProcessMemory` para alterar o campo `strength` do Haaland
  (playerid 239085) de 90 para 99 diretamente nesse blob:
  - A escrita **persiste** na memória (relendo depois, confirma 99).
  - **Porém não reflete na tela de Perfil do Jogador** (nem na
    primeira vez que o usuário abriu a tela do Haaland nesta sessão,
    nem reabrindo a tela do Ibrahim Mbaye já vista antes).
  - **Também não reflete na busca de transferências** (filtro por
    Strength alto não trouxe o Haaland).
  - **Buscando todas as ocorrências da string `"CZUM"` em toda a
    memória** (`find_all_czum.py`, novo script): 477 ocorrências
    espalhadas em várias regiões grandes de heap, mas **só uma**
    delas faz parte de um cabeçalho de database completo válido — as
    outras são provavelmente referências avulsas ao nome da tabela
    usadas por outros subsistemas (não bancos de dados completos
    alternativos).

**Conclusão**: esse blob de banco de dados no heap é quase certamente
um **snapshot somente-lido-uma-vez** (carregado no início da sessão
de carreira, talvez para inicializar os objetos "vivos" de cada
jogador, ou para operações internas que escaneiam todos os jogadores
de uma vez, tipo geração de regens/scouting em segundo plano) — não é
a fonte que a UI ou a lógica de busca consultam continuamente. Os
dados realmente "vivos" (que a UI lê) parecem estar em objetos de
heap separados, um por jogador **atualmente carregado/referenciado**
pelo motor — o mesmo tipo de estrutura alcançada pelas cadeias de
ponteiros da Cheat Table (`E0 -> 120`), não um array contíguo.
Escrever nesse blob não tem efeito observável no jogo.

**Não testado ainda** (possível próximo passo se quiser insistir nessa
linha): forçar o jogo a *recarregar* a partir desse blob de alguma
forma (ex: sair completamente da carreira e voltar sem fechar o
processo, o que talvez dispare um novo "load" que leia esse blob de
novo) — mas isso não ajudaria a EDITAR em lote, só confirmaria a
hipótese de quando ele é lido.

### Ferramentas novas criadas nesta sessão (todas em `fifa_process_identifier/`, somente leitura exceto `test_memory_write.py`)

- `resolve_chain.py`: resolve uma cadeia de ponteiros arbitrária no
  estilo Cheat Engine (`fifa16.exe+offset -> offset1 -> offset2 -> ...`),
  com dump de bytes ao redor do endereço final. Reutilizável para
  qualquer entrada de `cheat_table_parsed.txt`.
- `scan_cluster.py`: dado um conjunto de valores int32 conhecidos (ex:
  atributos de um jogador visível na tela), varre toda a memória
  PRIVATE+COMMIT do processo procurando "clusters" onde vários desses
  valores aparecem próximos uns dos outros — útil para achar structs
  sem precisar de pointer scan completo. Requer `numpy` (instalado via
  pip nesta sessão, não estava presente antes).
- `find_db_in_memory.py`: varre toda a memória do processo procurando
  a assinatura de database FIFA (`DB\x00\x08\x00\x00\x00\x00`) —
  encontrou o blob completo de 38 tabelas descrito acima.
- `find_all_czum.py`: varre toda a memória procurando a string curta
  `"CZUM"` (não só cabeçalhos de DB completos) — usado para confirmar
  que só existe uma cópia completa da database, não múltiplas.
- `find_crc_table.py` / `find_crc_xref.py`: análise estática do
  executável (procura tabela CRC32 padrão embutida, tenta localizar
  xrefs via parsing manual do PE header) — confirmou a tabela existe
  mas revelou o problema do packer (seções com layout de arquivo
  inconsistente).
- `test_memory_write.py`: **único script desta sessão que ESCREVE em
  memória** (`WriteProcessMemory`). Usado só para o teste controlado
  documentado acima. Requer `process.open_process_write()` (nova
  função adicionada a `process.py` nesta sessão, com
  `PROCESS_VM_WRITE | PROCESS_VM_OPERATION`) — antes só existia
  `open_process()` somente leitura.
- Scripts de teste de checksum (`test_crc.py` até `test_crc6.py`,
  `find_crc_table.py`, `find_crc_xref.py`) foram criados em
  `C:\Users\Felipe\AppData\Local\Temp\opencode\` (fora do repo,
  conforme convenção de arquivos temporários) — não persistidos no
  projeto, mas a lógica/resultados estão documentados aqui caso
  alguém queira refazer.

### Estado da questão "escrita em lote" ao final desta sessão

Nenhum dos dois caminhos planejados (arquivo via checksum, memória via
blob de database) provou ser viável para edição em lote sem grandes
esforços adicionais:

- **Arquivo/checksum**: bloqueado por anti-tamper. Só resolvível com
  engenharia reversa dinâmica de alto risco (dump de memória
  desempacotada, contornar anti-debug) — não tentado por causa do
  risco de crash/possível dano ao save real do usuário, e por ser
  fora do escopo de uma sessão de investigação.
- **Memória/blob CZUM**: escreve com sucesso mas não tem efeito
  observável no jogo (não é a fonte viva).
- **Memória/cadeias de ponteiro por jogador (Cheat Table)**: teoricamente
  funcional (é a técnica usada pelo Live Editor da comunidade), mas
  é fundamentalmente per-jogador-com-tela-aberta — não escala para
  lote sem automação de UI (abrir/fechar tela de cada jogador
  programaticamente), que é um projeto separado de automação de
  input, não só leitura/escrita de memória.

## Escrita em memória — sessão 4: SUCESSO CONFIRMADO para campos globais de carreira

Sessão focada em resolver escrita de verdade (usuário pediu para não
avançar com features antes de validar). Retomou tentativas de achar a
estrutura "viva" de atributos de jogador (sem sucesso — ver abaixo) e
depois pivotou para um alvo mais simples: orçamento do clube, motivado
pela prioridade real do usuário (Scout > Treinamento, na ordem
Treinamento > Scout > bug "de Oliveira").

### Atributos de jogador (per-player) — ainda NÃO resolvido

Tentativas nesta sessão, todas malsucedidas (escreve mas não reflete
na UI, ou reflete mas não persiste):

1. **Diff de memória entre dois jogadores na tela de Perfil** —
   achou 18 endereços que mudam consistentemente com o playerid
   selecionado, mas ao escrever neles o valor "volta sozinho" para
   outro número ao trocar de tela — são buffers efêmeros de
   renderização da UI (Scaleform/GFx), recicláveis a cada frame/tela,
   não o dado persistente.
2. **Mesma técnica na tela de Edição** (chuteira/luva/camisa —
   confirmado pelo usuário que é a mesma tela onde a Cheat Table
   funciona fora do Modo Carreira) — mesmo resultado: endereços
   encontrados (ex: `0x8E8C7C90-48` para overall, valores de
   Reactions/Strength em offsets vizinhos) escrevem com sucesso
   (releitura confirma) mas não refletem na tela nem sobrevivem a
   troca de tela.
3. **Cadeias de ponteiro da Cheat Table v7.1** (`fifa16.exe+0x34D7908
   -> ...`) — continuam retornando ponteiro nulo mesmo na tela de
   Edição correta. A versão do executável do usuário é
   `16.0.2904053`; a CT provavelmente foi feita para outra
   build/plataforma e os offsets não batem.
4. **Blob completo da database `CZUM` no heap** (achado na sessão 3)
   — teste definitivo desta sessão: escreveu `strength=99` num
   jogador, usuário salvou o jogo pelo menu normal, e o save
   resultante manteve o valor original (43). Confirma que esse blob
   é mesmo um snapshot desconectado da fonte real usada para salvar
   — **não usar mais esse caminho para atributos de jogador**.
5. **Descoberta lateral interessante**: existem marcadores de debug
   no heap no formato `<table_shortname><field_shortname>` (8 bytes
   ASCII, ex: `"CZUMnmgT"` para `CZUM.strength`), seguidos de
   valores com padrão `0xCDCDCDCD` no dword alto (canary de heap de
   debug do MSVC). Encontrados via `find_field_accessor.py`. Parecem
   ser contadores/instrumentação de acesso a campo, não os valores
   reais — não geraram um caminho de escrita funcional. Só aparecem
   quando aquele campo específico está sendo ativamente
   exibido/monitorado por uma tela (não achamos ocorrência para
   campos de tabelas fora do contexto atual, ex: `dqXv.TAIb` não
   apareceu).
6. **Pointer scan reverso ao vivo** (`live_pointer_scan.py`, novo
   script) partindo dos endereços de atributo "vivos" candidatos —
   não achou nenhum ponteiro (nem em heap nem em módulo) apontando
   diretamente para esses offsets, sugerindo que são campos no meio
   de um nó, não o início de uma alocação referenciada diretamente.

**Conclusão sobre atributos de jogador**: a estrutura real usada pela
UI para exibir/editar atributos individuais ainda não foi localizada.
É bem provável que exista e seja alcançável por uma cadeia de
ponteiros nos moldes da Cheat Table, mas descobri-la do zero exigiria
um pointer-scan multi-nível sistemático (BFS completo, não apenas
1-2 níveis manuais) — esforço não trivial, pausado por decisão do
usuário a favor de focar em Scout primeiro.

### Orçamento do clube (transferbudget) — SUCESSO CONFIRMADO, técnica reutilizável

Ao contrário dos atributos de jogador, o campo `dqXv.transferbudget`
(orçamento de transferência, tabela `dqXv` = career_options/settings,
1 único registro) foi escrito com sucesso e **confirmado
persistindo em um save real**:

1. Valor exibido na tela (Modo Carreira > Transferências): `166.870.000`.
2. `scan_value_live.py <valor>` (novo script — busca simples por int32
   exato em regiões PRIVATE+COMMIT) encontrou **1 única ocorrência**:
   `0x8D7F07CC` (endereço de heap, muda entre sessões — precisa
   re-escanear toda vez).
3. Estabilidade confirmada por 5 segundos antes de escrever.
4. `WriteProcessMemory` gravou `500000000` nesse endereço.
5. **A tela não atualizou instantaneamente enquanto parado nela**,
   mas **ao trocar de tela e voltar, o novo valor apareceu
   corretamente** — diferente do comportamento efêmero visto nos
   atributos de jogador, aqui o valor realmente é a fonte persistente
   (a UI só não re-renderiza em tempo real sem um evento de
   navegação).
6. Usuário salvou pelo menu normal do FIFA. **Novo arquivo DATA
   gerado, parseado com sucesso** (não corrompido — checksum
   recalculado automaticamente pelo próprio jogo): `dqXv.transferbudget
   = 500000000` confirmado no save em disco.

**Isso é a primeira prova end-to-end funcional de escrita em memória
neste projeto**: memória → save real, sem tocar no checksum
manualmente. O padrão de sucesso parece ser: campos de estado
global/carreira (não por-jogador) são armazenados de forma mais
direta, sem passar pelo sistema de "accessor"/binding de UI complexo
usado para atributos de jogador individuais.

### Ferramentas novas desta sessão (`fifa_process_identifier/`)

- `find_selected_player_slot.py`: salva/compara scans de "qual
  endereço contém o playerid selecionado" entre duas capturas com
  jogadores diferentes na tela — usado para os testes malsucedidos de
  atributo, mas a técnica em si funciona bem para achar candidatos.
- `find_squad_array.py`: procura vários playerids conhecidos ao mesmo
  tempo e agrupa em "clusters" por proximidade — achou o array do
  relatório de elenco (agrupamento por faixa de overall, tabela de
  categoria/estrela, não os atributos brutos).
- `find_db_in_memory_v2.py`: como o da sessão 3, mas varre também
  regiões MEM_MAPPED e MEM_IMAGE (confirmou que só existe mesmo uma
  cópia da database completa, mesmo variando o tipo de região).
- `find_module_ptr_to_region.py`: varre as seções do próprio módulo
  `fifa16.exe` procurando ponteiros que caem dentro de um range de
  heap alvo — útil para achar variáveis globais/estáticas que
  referenciam uma região (achou `fifa16.exe+0x34D9530` apontando pro
  início exato do heap grande de objetos, mas não levou a uma cadeia
  navegável até jogadores individuais).
- `live_pointer_scan.py`: pointer-scan reverso multi-nível 100% ao
  vivo (sem precisar de snapshot em disco), reportando se algum
  ponteiro cai dentro do módulo exe (âncora estável). Reutilizável
  para retomar a busca de atributos de jogador no futuro.
- `find_field_accessor.py`: busca marcadores de debug
  `<table_shortname><field_shortname>` na memória — útil para
  investigar campos específicos, mas só funciona se aquele campo
  estiver "ativo" na tela atual.
- `scan_value_live.py`: **o script que efetivamente funcionou** —
  busca simples e direta por um valor int32 exato, alinhado a 4
  bytes, em toda a memória PRIVATE+COMMIT. Simples mas eficaz para
  valores suficientemente raros (ex: orçamento). Para valores comuns
  (ex: atributos 0-99) gera ruído demais sozinho — precisa combinar
  com outras pistas (ver `scan_cluster.py` da sessão 3).
- `resolve_chain.py` (sessão 3, já existia): usado para tentar validar
  a Cheat Table na tela de Edição — confirmou que os offsets da CT
  v7.1 não batem com a build atual do executável (`16.0.2904053`).

### Lição geral sobre metodologia de escrita em memória

Ficou claro nesta sessão que **nem todo endereço que "parece certo"
(bate com o valor esperado) é a fonte real de dados**. Antes de
confiar em qualquer endereço candidato, seguir sempre este checklist:

1. Ler o valor e confirmar que bate com o que está na tela.
2. Confirmar ESTABILIDADE: reler o mesmo endereço váááualumas vezes ao
   longo de alguns segundos SEM interagir com o jogo — se mudar
   sozinho, é buffer efêmero de UI, descartar.
3. Escrever um valor bem diferente do original (fácil de distinguir).
4. Verificar na tela SEM trocar de tela primeiro.
5. Se não refletir, trocar de tela e voltar (alguns campos só
   re-renderizam em eventos de navegação, não em tempo real — foi o
   caso do orçamento).
6. Só considerar "resolvido de vez" depois de um save real completo e
   confirmação de que o valor persistiu no arquivo em disco.

## Sessão 5 — Pivô para DLL injection: MVP de overlay funcionando

### Contexto da decisão

Sessão 4 conseguiu escrever com sucesso em memória (orçamento do
clube), mas atributos de jogador continuaram resistentes. Nesta
sessão, o usuário trouxe duas Cheat Tables adicionais (v7.1 já
conhecida, e um rascunho pessoal "Ethan1stDraft.CT") — ambas com o
mesmo endereço-base `fifa16.exe+0x34D7908`, mas nenhuma resolveu (o
valor nesse offset não é um ponteiro válido na build atual do jogo,
`16.0.2904053` — provavelmente as CTs foram feitas para outra
build/distribuição).

**Progresso real via scan manual + Cheat Engine**: usando o próprio
Cheat Engine (scan simples "Exact Value" + "Next Scan" filtrando por
troca de jogador — SEM pointer scan pesado), reduzimos de 4769 para 4
endereços candidatos para `strength`, e **confirmamos escrita bem
sucedida e persistente** em `0x8E8CF4B0` (dentro da região de heap
`0x8DAB0000`, a mesma onde mora o blob `CZUM`) — mudou de 74 para 20
e persistiu ao trocar de tela. Os outros 3 candidatos eram
coincidência/buffers reciclados.

**Crash confirmado ao usar "Pointer scan for this address" do Cheat
Engine**: essa operação suspende todas as threads do processo pra
tirar snapshot consistente — padrão clássico que watchdogs de
anti-tamper (Denuvo Anti-Tamper, presente desde FIFA 15/16) detectam
como debugger e reagem fechando o jogo. **Confirmado que o save não
corrompeu** (havia um autosave recente e válido). Esse é exatamente o
mesmo tipo de crash já documentado no M1 (sessão de investigação
original) com "Find out what accesses this address".

### Decisão: pivotar para DLL injection

Diante do padrão repetido (scans pesados de fora do processo
crashando o jogo), e com o usuário quermendo também um overlay em
fullscreen como feature futura, decidiu-se investir na abordagem mais
robusta: **DLL injetada rodando de dentro do processo**.

Pesquisa técnica (via agente) confirmou:
- FIFA 16 usa engine **Ignite** (não Frostbite — essa só entra a
  partir do FIFA 17) e **Denuvo Anti-Tamper** (não Denuvo Anti-Cheat,
  que não existe nessa geração). O foco da proteção é integridade de
  código + anti-debug, não comportamento online — relevante porque
  **não há telemetria remota de "trapaça"** para se preocupar, é
  puramente sobre não travar o jogo localmente.
- Uma DLL injetada, lendo memória via ponteiros diretos (sem
  `ReadProcessMemory` externo, sem suspender threads), é
  estruturalmente **mais segura** contra esse tipo de anti-tamper do
  que qualquer ferramenta externa (CE, scripts Python) — explica
  tecnicamente por que o crash aconteceu especificamente com pointer
  scan (suspensão de threads) e não com os scans simples de valor.
- Stack recomendada e adotada: **Rust** + `hudhook` (overlay: hook de
  `Present()` D3D11/D3D12/D3D9/OpenGL3 + integração ImGui, tudo
  pronto) + `dll-syringe`/`hudhook::inject` (injeção via
  `CreateRemoteThread`+`LoadLibrary` clássico — considerado seguro o
  suficiente para esse tipo de proteção, sem necessidade de manual
  mapping neste estágio).
- Para descoberta de estruturas de jogador (próxima fase, ainda não
  implementada): usar **AOB/pattern scanning** com wildcards em vez
  de offsets fixos (mais resistente a mudanças de build), navegar via
  `lea reg, [rip+imm32]` para resolver singletons, e potencialmente
  RTTI scanning / hook de função de acesso a atributo (técnica
  usada por ferramentas maduras tipo o Live Editor da comunidade,
  cujo código-fonte é fechado mas cuja arquitetura pública — API Lua
  documentada — confirma esse padrão).

### Setup de ambiente (novo nesta sessão)

- **Rust instalado via winget** (`Rustlang.Rustup`), toolchain
  `x86_64-pc-windows-msvc` — já havia MSVC Build Tools 14.51 instalado
  previamente no sistema (`C:\Program Files (x86)\Microsoft Visual
  Studio\18\BuildTools`), então não foi necessário instalar C++
  tooling adicional.
- **Dois crates Cargo criados na raiz do projeto**:
  - `fifa_overlay/` — biblioteca `cdylib`, é a DLL injetada. Depende
    de `hudhook` (feature `dx11`), `imgui`, `tracing` +
    `tracing-subscriber` + `tracing-appender` (log em arquivo, já que
    não há console visível dentro do processo do jogo — grava em
    `%TEMP%\fifa_overlay.log`).
  - `fifa_injector/` — binário simples que usa
    `hudhook::inject::Process::by_name("fifa16.exe").inject(dll_path)`
    pra injetar a DLL. Aceita caminho da DLL como argumento opcional
    (default: procura `fifa_overlay.dll` ao lado do próprio `.exe`).

### MVP confirmado funcionando end-to-end

1. `cargo build --release` em ambos os crates compila sem erros
   (só 1 warning inofensivo do linker sobre mensagem em
   português/codepage, não afeta funcionamento).
2. Com o FIFA 16 rodando, `fifa_injector.exe
   ...\fifa_overlay\target\release\fifa_overlay.dll` injeta com
   sucesso (`Process::by_name` + `inject` — sem crash).
3. Log da DLL (`%TEMP%\fifa_overlay.log`) confirma
   `IDXGISwapChain::Present trampoline` sendo chamado a cada frame
   (~22 FPS observado nos timestamps) — o hook está ativo e estável.
4. **Confirmado visualmente pelo usuário**: uma janela ImGui
   ("FIFA 16 Companion", texto "Overlay funcionando!" + contador de
   frames) aparece sobreposta à tela do jogo em tempo real.
5. Jogo permaneceu estável, sem qualquer sinal de detecção do
   anti-tamper durante o teste.

**Isto é a primeira validação completa do pipeline DLL injection +
overlay para este projeto.** Não houve ainda nenhuma leitura/escrita
de memória do jogo a partir da DLL — é puramente a infraestrutura de
overlay, que era o passo 1 combinado com o usuário (validar a base
antes de partir para engenharia reversa de estruturas de jogador).

### Leitura de memória in-process — SUCESSO CONFIRMADO (mesma sessão 5)

Implementado e validado end-to-end antes mesmo de partir para pattern
scanning avançado: um scanner de memória rodando DENTRO da DLL,
reimplementando em Rust a mesma lógica de `find_db_in_memory.py` +
`fifa16_db_parser.py`.

**Dois bugs de segurança de memória encontrados e corrigidos no processo**
(documentados aqui porque são lições reutilizáveis para qualquer
trabalho futuro de leitura de memória in-process):

1. **Bug 1 — bloqueio de UI**: a primeira implementação chamava a
   função de scan diretamente dentro de `ImguiRenderLoop::render`
   (callback do hook do `Present`). Como o scan usava um loop de busca
   de substring ingênuo (byte-a-byte manual) sobre potencialmente
   centenas de MB de memória, o scan levou **2 minutos e 10
   segundos**, bloqueando a thread de render inteira (a mesma thread
   que desenha o jogo) — apareceu como "jogo travado" para o usuário
   (áudio/input continuavam funcionando, só o render travava).
   **Corrigido em duas frentes**:
   - Trocada a busca de substring ingênua por `memchr::memmem`
     (crate `memchr`, usa SIMD quando disponível) — reduziu o tempo de
     ~2min para ~17 segundos para a mesma varredura completa.
   - O scan foi movido para uma **thread separada** (`std::thread::spawn`),
     com o resultado compartilhado de volta para a thread de render via
     `Arc<Mutex<ScanState>>` + `Arc<AtomicBool>` (flag de "scan em
     progresso"). A UI agora mostra "Escaneando... (rodando em
     background)" enquanto isso, sem travar nada.

2. **Bug 2 — crash real (access violation)**: ao mover o scan para uma
   thread separada, o jogo passou a **crashar de verdade** (sem gerar
   novo `crash.dmp`, confirmando que não era o anti-tamper e sim uma
   exceção de hardware não tratada). Causa raiz: a primeira versão do
   scanner lia memória desreferenciando um ponteiro Rust cru
   (`std::slice::from_raw_parts`) sobre o endereço confirmado por
   `VirtualQuery` — mas como agora o scan roda CONCORRENTEMENTE com o
   motor do jogo (que continua alocando/liberando memória em tempo
   real, especialmente com uma carreira carregada), uma página podia
   ser desmapeada pelo próprio FIFA entre o `VirtualQuery` e a leitura
   real, causando `STATUS_ACCESS_VIOLATION` — uma exceção de hardware
   que `catch_unwind` do Rust NÃO consegue capturar (não é um panic).
   **Corrigido trocando a leitura para `ReadProcessMemory` com o
   pseudo-handle do próprio processo (`GetCurrentProcess()`)**: essa
   API do Windows faz a cópia de forma protegida internamente pelo
   kernel — se a página não estiver mais acessível no momento exato
   da cópia, retorna `FALSE` (erro tratável) em vez de crashar. Mesma
   técnica/princípio já usado nas ferramentas Python externas
   (`ReadProcessMemory` via `ctypes`), só que aqui aplicada a
   self-process memory reading. **Custo**: a função agora retorna
   `Vec<u8>` (cópia owned) em vez de `&'static [u8]` (referência à
   memória original) — correto e necessário, já que a memória
   original pode virar lixo a qualquer momento de qualquer forma.

**Resultado final validado pelo usuário**: com o Modo Carreira
carregado, scan em background completa em ~17s, encontra as
databases em memória (9 localizações, mesma ordem de grandeza do
Python), localiza o jogador de teste (Ibrahim Mbaye, playerid 74449)
na tabela CZUM, e exibe no overlay ImGui valores que **batem
exatamente** com o esperado: `Strength=43, Overall=82, Potential=81`.
Jogo permaneceu 100% estável durante todo o processo, sem nenhuma
reação do anti-tamper (confirma a hipótese da pesquisa: leitura
via ponteiros/API local, sem suspender threads, é estruturalmente
segura contra esse tipo de proteção).

Arquivos novos: `fifa_overlay/src/memscan.rs` (scanner de memória +
leitura protegida) e `fifa_overlay/src/fifa_db.rs` (parser mínimo do
formato t3db v8, sem metadata XML nem decodificação Huffman — só
campos inteiros via `read_packed_int`, suficiente para atributos
numéricos).

### Pointer scan reverso in-process — implementado, funcional, mas com muito ruído

Continuação da sessão 5 (mesmo dia): implementado `pointer_scan.rs`,
um BFS multi-nível 100% in-process (sem `OpenProcess`/`ReadProcessMemory`
externo, sem suspender threads) para tentar achar uma cadeia de
ponteiros estável a partir de um endereço "vivo" conhecido — a mesma
técnica que o Cheat Engine faz com "Pointer scan for this address",
mas de forma segura (o CE crashou o jogo ao suspender threads; nossa
versão in-process não deveria ter esse problema).

**Fluxo de trabalho estabelecido**: usar o Cheat Engine só para
scans de valor simples (`Exact Value` + `Next Scan`, filtrando por
troca de jogador na tela de edição) — isso é rápido e nunca causou
crash. Uma vez obtido um endereço "vivo" candidato (ex: `0x8E8CF4B0`,
que se mostrou surpreendentemente **estável entre reinícios do jogo**
nesta sessão — mesmo endereço encontrado em 3 sessões diferentes),
alimentar esse endereço na função `find_pointer_chain` da DLL.

**Resultado**: o BFS funciona (não crasha, roda em thread separada,
usa `ReadProcessMemory` protegido), mas gera uma quantidade de ruído
que cresce descontroladamente a cada nível — ex: nível 1 com 6-38
hits (variável entre execuções do mesmo scan!), nível 2 já com
dezenas a centenas, nível 3-4 com milhares. Em nenhuma tentativa uma
âncora dentro de `fifa16.exe`/`fifa16.bin` foi encontrada dentro de 4
níveis. **Descoberta importante**: o número de hits de nível 1 variou
entre execuções consecutivas do mesmo scan sem trocar nada no jogo
(6 → 36 hits para o mesmo endereço-alvo) — isso indica que a maioria
desses "ponteiros" são coincidências numéricas do heap dinâmico (o
motor do jogo aloca/libera memória constantemente), não referências
reais e estáveis. Pointer-scan por força bruta, sem uma forma de
filtrar heap "lixo" vs. heap "estrutural", não é uma abordagem
confiável neste jogo/engine.

**Mitigação parcial implementada**: a função retorna também os hits
"crus" do nível 1 (`level1_hits`) para inspeção manual mesmo se o BFS
completo não achar uma âncora, e um limite anti-explosão
(`MAX_TARGETS_PER_LEVEL = 2000`) corta o crescimento a cada nível.

### Teste de escrita no blob CZUM a partir da DLL — CONFIRMADO SEM EFEITO (mesma conclusão da sessão 4)

Implementado `write_bytes_at` (via `WriteProcessMemory` protegido) e
`test_write_strength` — escreve `strength=99` no registro do jogador
de teste (Ibrahim Mbaye, playerid 74449) diretamente no blob CZUM,
usando a mesma lógica de bit-packing de `write_packed_int` do Python
(reimplementada em `fifa_db::build_packed_bytes`/`locate_packed_field`).

**Resultado**: a escrita reporta sucesso (`ok=true`, confirmado no
log) e persiste (releitura confirma o novo valor), mas **a tela de
Perfil do jogador continua mostrando o valor original (43)** — exatamente
a mesma conclusão da sessão 4 (testada então via ferramenta Python
externa). Isso **confirma definitivamente, agora também a partir de
dentro do processo**, que o blob CZUM é uma cópia desconectada da
fonte real usada pela UI — não adianta insistir em escrever nele por
nenhum caminho (externo ou in-process).

### Bugs de infraestrutura encontrados nesta sessão (importantes para qualquer trabalho futuro com a DLL)

1. **Arquivo `.dll` "preso" no disco após crash/fechamento do jogo**:
   em várias ocasiões, `cargo build --release` reportava sucesso
   (`Finished ... target(s)`) mas **silenciosamente falhava em
   substituir o arquivo antigo** (mensagem de erro do linker/cargo
   aparecia só depois, como `failed to remove file ... Acesso negado`,
   e às vezes nem isso). Isso levou a testar builds antigas por engano
   várias vezes, achando que mudanças de código não tinham efeito.
   **Diagnóstico**: o Windows mantém um lock de "delete" sobre uma DLL
   enquanto ela está mapeada como imagem executável (`SEC_IMAGE`) em
   ALGUM processo — mesmo depois de fechar esse processo, se outro
   processo (ou o mesmo, reaberto) ainda referenciar aquele mapeamento
   de alguma forma, ou em cenários de timing raros, o handle pode não
   ser liberado imediatamente. `Remove-Item`/delete falha com "acesso
   negado" (não "arquivo em uso", mensagem enganosamente genérica),
   mas **`Rename-Item` funciona** mesmo com o lock ativo — útil como
   workaround: sempre que suspeitar desse problema, tentar renomear o
   `.dll` antigo antes de rebuildar. **Lição**: sempre verificar a
   string/conteúdo do binário recém-compilado (ex: `Select-String` por
   um texto novo do código) antes de gastar tempo depurando
   "comportamento estranho" que na verdade é só build desatualizada.
2. **`hudhook::eject()` nem sempre libera a DLL de forma limpa/rápida**:
   tentamos várias vezes usar eject + reinjeção para evitar reiniciar
   o jogo inteiro a cada mudança de código. Funcionou pelo menos uma
   vez de forma limpa, mas em outras ocasiões pareceu deixar o
   processo num estado inconsistente (overlay não atualizava,
   possivelmente por conflito de estado global do MinHook ao
   reinicializar hooks sem um unhook completo). Quando isso acontece,
   fechar e reabrir o FIFA do zero sempre resolve — não vale a pena
   insistir em debugar o eject quando o sintoma aparecer, é mais
   rápido reiniciar o jogo.
3. **Bug de borrow checker Rust** (não runtime, pego em compile-time):
   `MutexGuard` de `scan_state` ficava vivo durante todo o resto do
   closure de render por causa de `let state = self.scan_state.lock()`
   sem escopo próprio, impedindo chamar `&mut self` em código
   posterior no mesmo closure (ex: `self.test_write_strength(...)`).
   Corrigido envolvendo o `match` num bloco `{ ... }` extra para
   dropar o guard antes do código seguinte.
4. **Bug de segurança de memória (panic em runtime, não crash de
   hardware)**: `&bytes[a..b]` (indexação direta) em vez de
   `bytes.get(a..b)` pode gerar `panic!` do Rust (out of bounds) se o
   registro calculado cair fora do buffer lido — diferente do
   `STATUS_ACCESS_VIOLATION` de hardware (esse SIM pode ser
   capturado/evitado, já que é um panic Rust normal, mas ainda assim é
   melhor evitar com `.get()` retornando `Option`). Corrigido em
   `test_write_strength`.

### Estado ao final da sessão 5 — infraestrutura sólida, escrita de atributo de jogador ainda não resolvida

**O que temos de sólido e reutilizável**:
- Pipeline de build + injeção + eject funcionando (`fifa_overlay` +
  `fifa_injector`, ambos em Rust/Cargo na raiz do projeto).
- Leitura de memória in-process robusta e rápida (scan de banco de
  dados completo em ~17s, sem crashes, via `ReadProcessMemory`
  protegido).
- Scanner de valor exato in-process (`scan_for_i32_value`) e filtro
  incremental (`filter_addresses_by_i32_value`, equivalente a "Next
  Scan" do CE) — não usados no fluxo final desta sessão (decidiu-se
  usar o CE externo para essa parte, mais rápido/maduro), mas ficam
  disponíveis no código.
- Pointer-scan reverso in-process funcional mas com muito ruído (ver
  acima) — não chegou a uma âncora estável ainda.
- Escrita protegida (`write_bytes_at`/`WriteProcessMemory`)
  confirmada funcionando tecnicamente, mas escrever no blob CZUM
  definitivamente não afeta o jogo real (conclusão agora dupla —
  sessão 4 externamente, sessão 5 in-process).

**O que ainda falta** (não resolvido nesta sessão): encontrar a
estrutura "viva" real de atributos de jogador. O endereço
`0x8E8CF4B0` (achado via CE nas sessões 4 e 5) demonstrou ser gravável
E refletir na tela de edição pelo menos uma vez (sessão 4) — mas o
pointer-scan a partir dele não convergiu para uma âncora confiável.

### Próximos passos sugeridos para retomar (não implementados)

Em ordem de promessa/esforço:

1. **RTTI/vtable scanning** (mencionado na pesquisa técnica, não
   tentado ainda): em vez de pointer-scan cego, escanear a heap
   procurando objetos cujo primeiro qword (vtable pointer) aponte
   para uma vtable de uma classe relevante (ex: `Player`,
   `FieldPlayer`) — exigiria primeiro achar essa vtable via RTTI
   (`.rdata` do binário) ou heurística. Mais complexo de implementar
   mas potencialmente muito mais confiável que pointer-scan por
   coincidência de valor.
2. **Investigar o dump de bytes ao redor dos hits de nível 1** (não
   feito): antes de descartar o pointer-scan, valeria inspecionar o
   conteúdo ao redor de cada um dos 6-38 candidatos de nível 1 —
   talvez um deles esteja dentro de uma struct reconhecível (com
   outros campos de jogador vizinhos, como fizemos manualmente nas
   sessões 3-4) mesmo sem levar a uma âncora em módulo.
3. **Reduzir ruído do pointer-scan filtrando por alinhamento/contexto
   de heap**: por exemplo, só considerar ponteiros que estejam eles
   mesmos dentro de regiões de heap "pequenas" (não a região gigante
   de 0x1200000 bytes onde mora o blob CZUM) — objetos individuais de
   jogador provavelmente vivem em allocations menores e mais
   estáveis.
4. **Dump de memória desempacotada + Ghidra/IDA estático** (esforço
   maior, mencionado na pesquisa): analisar o binário desempacotado
   offline para achar via AOB scanning a função que o jogo usa
   internamente para ler/escrever atributos (ex: `Player::GetAttribute`),
   e hookar essa função em vez de tentar achar o dado por
   pointer-scan — replicaria a técnica que ferramentas maduras tipo
   o Live Editor da comunidade usam de verdade.
5. Caso nenhuma dessas avance, considerar aceitar a limitação e focar
   nas features que JÁ são viáveis com a infraestrutura atual (Scout,
   leitura combinada de tabelas do save, escrita de campos globais
   tipo orçamento) enquanto se retoma escrita de atributos de jogador
   em uma sessão futura dedicada.

## Sessão 6 — Story 1.1: leitura de estado da carreira

Primeira sessão no Windows depois do código da Story 1.1 ter sido
escrito num Mac sem compilar. O crate `fifa_overlay` compilou de
primeira; 22 testes unitários passam (`cargo test`).

### Achados offline (sem o jogo aberto) — confirmados

- **Short names** (via `tools/resolve_short_names.py` +
  `fifa_ng_db-meta.xml`): `GJUr` = `career_calendar` (`currdate`=`aLZZ`,
  `startdate`=`vHhZ`, 19 bits, `rangelow=20080101`); `mPrV` =
  `career_users` (`firstname`=`HdeP`, `surname`=`rREd`,
  `clubteamid`=`NTyS` com `rangelow=-1`); `dqXv` = `career_managerpref`
  (`transferbudget`=`SnDr`, 31 bits). As datas ficam guardadas como
  `YYYYMMDD - 20080101`; depois de somar o `rangelow` o valor é o
  `YYYYMMDD` normal.
- **Nomes do manager NÃO são Huffman**: `firstname`/`surname` são
  strings fixas inline (`storage_type` 0, 256 bits = 32 bytes,
  terminadas em `\0`). AD-11 vale sem desvio.
- **O save tem 3 databases**: carreira (34 tabelas: `GJUr`/`mPrV`/
  `dqXv`...), jogadores (38 tabelas, `CZUM` com 32.602 registros) e uma
  com 1 tabela. O código original da story exigia `GJUr` E `CZUM` no
  mesmo blob — **nunca casaria**. Corrigido: `is_career_db` agora exige
  `GJUr` + `mPrV` + `dqXv`.
- **Build do jogo**: o `FileVersion` fixo do `fifa16.exe` é `1.0.0.0` e
  o `ProductVersion` fixo é `16.0.0.0`; só a **string** `ProductVersion`
  (`StringFileInfo`) traz `16.0.2904053` (2904053 nem cabe em 16 bits).
  `save_repo` agora compara essa string; testado contra o exe instalado.
- **Identidade (AD-11)** nos saves de `save_backups/`:
  `717036e3` e `705c22c4` → `20350717|Felipe|Careca|241` (mesma
  carreira, dias diferentes → mesmo hash); `7a096416` →
  `20350721|Felipe|Careca|73` (outra carreira → hash diferente). Os
  saves atuais em `Documents\FIFA 16` têm outras duas carreiras
  (`20280715|...|234` e `20270108|...|130285`). Teste de oráculo:
  `save_repo::tests::real_saves_*`.

### Teste com o jogo aberto (carreira "teste", 2026-09-30) — blob de carreira NÃO é fonte viva

O FIFA roda **elevado** (admin): o injetor tem de rodar num terminal
como Administrador ("Acesso negado 0x80070005" caso contrário).

1. Logo após criar a carreira e salvar (save 21:42: `transferbudget =
   74.000.000`), o usuário passou dinheiro do orçamento de transferência
   para o de salários **sem salvar** (tela: 67.000.000).
2. "Localizar carreira": build verificada (`16.0.2904053`), **1 único**
   blob de carreira (34 tabelas, `0x8B470000+0x352D4`), `currdate =
   20260701` (bate com a tela), identidade `20260717|Senhor|Manager|243`,
   mas **`transferbudget = 74.000.000`** — o valor do último save, não o
   da tela.
3. Usuário salvou de novo (DATA no disco: `transferbudget = 67.000.000`,
   `wagebudget` 500.000 → 634.615). Duas novas localizações **não
   acharam mais nenhum blob de carreira** no heap.

**Conclusão**: o blob de carreira no heap é um buffer de
carga/serialização do save (conteúdo = último load/save), não o estado
vivo, e é liberado/realocado depois de um save — mesma natureza do
blob `CZUM`. **Não serve como fonte de `currdate`/`transferbudget`**,
e nem é confiável para a identidade (pode não existir na hora).
A fonte viva do orçamento continua sendo a ocorrência única achada por
scan de valor exato (sessão 4). Decisão de estratégia (Task 1.3)
pendente com o Felipe.

### Sonda de estado vivo — struct viva do orçamento ACHADA (2026-09-30)

Felipe escolheu a estratégia "sondar struct viva". Botão "Sondar" na
janela de debug (`save_repo::start_live_probe`): scan do orçamento
atual por i32 exato + procura de outros valores da carreira em ±1 KB.
Obs. (atualizada): o eject + reinjeção funcionou ~4 vezes seguidas e
na 5ª o **jogo crashou** logo após o "Finished removing hook" (antes
da DLL nova inicializar) — confirma o item 2 dos "Bugs de
infraestrutura". Use eject por conveniência, mas conte com reiniciar.
Para reinjetar a build mais recente: `fifa_overlay\inject_dev.ps1`
(como Administrador). Log reduzido para INFO (o TRACE do hudhook
gravava uma linha por frame).

Obs.: "Descarregar DLL (eject)" + injetar de novo **funcionou** desta
vez sem reiniciar o jogo.

Orçamento na tela `63.999.988` (salário `692.307`): 71 ocorrências; 69
soltas; duas com vizinhos. A melhor (`0x8CD9E74C`, região
`0x8CCB0000`) é o `dqXv` vivo decodificado em i32 contíguos:

| offset | valor | campo |
| --- | --- | --- |
| −4 | 692.307 | `wagebudget` (atual) |
| 0 | 63.999.988 | `transferbudget` (atual) |
| +12 | 932 | ? |
| +16 | 1 | ? |
| +20 | 4.650.000 | `startofseasonwagebudget` |
| +24 | 74.000.000 | `startofseasontransferbudget` |
| +28 | 4.150.000 | `startofseasonplayerwages` |
| +32 | 38.000 | ? |

A segunda (`0xAE8C0F08`) tem a mesma ordem com passo de 16 bytes
(−16 salário, 0 orçamento, +80/+96/+112 valores de início de temporada)
— outra representação interna.

**Consequência**: dá para localizar o orçamento vivo SEM saber o valor
atual: os três valores de início de temporada são constantes na
temporada e estão no save em disco → procurar o padrão contíguo
`[startwage, starttransfer, startplayerwages]` e ler o orçamento 24
bytes antes. A data (`GJUr`) não está perto; precisa de outra sonda.

### Localização por assinatura — FUNCIONANDO (orçamento vivo)

`save_repo::locate` lê os saves recentes em `Documents\FIFA 16`, procura
na memória o trio de início de temporada e lê o orçamento 20 bytes
antes. 1º teste falhou: a DLL achou a assinatura na PRÓPRIA lista de
saves (`0x5529D4`, entradas a cada 120 bytes) — corrigido excluindo a
memória da DLL (lista, pilha, buffer único de leitura) e zerando as
cópias. 2º teste: orçamento e salário vivos batendo com a tela e
acompanhando mudanças sem relocalizar.

### Data viva — candidatos (2026-09-30)

- Sonda com âncora na data (`20260703`/`20260705`): 107–178 cópias,
  nenhuma com os campos fixos de `GJUr` (startdate, enddate,
  objectivecheckdate, setupdate, janelas de transferência) a ±1 KB →
  a data viva NÃO está numa cópia decodificada de `GJUr`.
- Scan de valor (CE e DLL) ao longo de vários dias: 3 endereços que
  acompanham o calendário: `0x8CFA3E08` (campo isolado num objeto com
  ponteiros para o mesmo bloco — melhor candidato), `0x8CE02BD4` e
  `0x8CE03114` (listas de eventos com registros de 32 bytes).
- **Layout repetido entre sessões**: struct do orçamento em
  `0x8CD9E74C` numa sessão e `0x8CD1E74C` na outra (+`0x80000`), mesmo
  offset `+0xEE74C` dentro da região. Hipótese: data =
  orçamento + `0x2856BC` (ou `+0xE4488`/`+0xE49C8`). Implementado com
  validação (data entre o último save e `GJUr.enddate`) e queda para a
  data do último save.
- **Validado numa sessão nova (s6-v7)**: orçamento de novo em
  `0x8CD9E74C`; `+0x2856BC` e `+0xE49C8` = `20260630` logo após
  carregar o save de 1/jul (um dia atrás), e depois de avançar dias o
  campo bateu exatamente com a tela (`20260703` = 3/jul). `+0xE4488`
  tinha lixo (`20240731`). Ajuste (s6-v8): aceitar o dia anterior ao
  save e, nesse caso, devolver a data do save.
- Injeção: v7 crashou o jogo 2× via `inject_dev.ps1` e carregou 1× pelo
  injetor direto (mesmo binário). Causa não identificada — pode ser
  corrida intermitente do hook do hudhook com o `Present` do jogo, não
  necessariamente o script.

### Troca de carreira na mesma sessão (s6-v8) — restos na memória

Com "teste" carregada antes e depois a carreira Careca (save
`17d87c6a`, 23/set/2028): a localização achou DUAS structs — a de
"teste" intacta em `0x8CD9E74C` (resto, não liberada) e a da Careca em
`0x8CD9AD3C` (região +`0xEAD3C`) — e escolheu "teste" (save mais
recente). As 3 posições de data (relativas à struct de "teste") liam
`20280923` = data da CARECA → a data fica num ponto fixo do bloco
(região +`0x373E08`), não acompanha a struct de finanças.

Correção (s6-v9): data lida de região + `0x373E08` (reservas
+`0x1D2BD4`/+`0x1D3114`); a carreira ativa é a struct cuja temporada
(até 366 dias antes do `enddate`) contém a data viva; validação da data
pela janela da temporada (o jogador pode carregar um save mais antigo).

s6-v9 confirmada: com a Careca carregada escolheu a Careca (ignorou o
resto de "teste"). Localização não engasga o jogo (FPS ok).

### Menu principal (s6-v9) — data principal também é resto

No menu, sem carreira: região +`0x373E08` ainda tinha `20280923` (data
da última carreira) e as listas de eventos (+`0x1D2BD4`, +`0x1D3114`)
estavam **zeradas** → v9 "confirmou" a Careca. Correção (s6-v10):
data viva só vale se **2 das 3 posições concordam**; sem carreira
confirmada → `CarreiraNaoCarregada` (removida a queda para o save mais
recente); toda leitura revalida a data (menu → "carreira não
carregada"; data que VOLTA → outro save carregado → cache descartado).

### Ainda pendente (Story 1.1)

1. ~~Confirmar s6-v10~~ **confirmado**: menu → `CarreiraNaoCarregada`;
   carreira → valores vivos. Story 1.1 → review. Plano B: pointer scan do CE na data. Duas carreiras na MESMA
   temporada não são distinguíveis pela data (vence o save mais
   recente). Recarregar o MESMO save na mesma data pode deixar o cache
   apontando para o resto da carga anterior — relocalizar resolve.
2. Protocolo AC #3 em 3 sessões (mesma carreira 2×, outra 1×).
3. Menu sem carreira carregada → `CarreiraNaoCarregada`; FPS liso
   durante a localização.

## Sessão 7 — Story 1.2: painel da Central de Scout (2026-10-01)

- `fifa_overlay` agora é só o produto: F10 abre/fecha a Central de Scout
  (tema do DESIGN.md, 4 abas, cabeçalho com orçamento/data vivos, estados
  vazios). Validado no jogo pelo Felipe.
- A janela de diagnóstico (sonda, scans, teste de escrita) foi arquivada em
  `fifa_overlay_debug/` (compila e injeta sozinha; log
  `fifa_overlay_debug.log`). Não injetar as duas juntas.
- Fontes: Oswald (estática) e Inter (variável) do Google Fonts, OFL, em
  `fifa_overlay/assets/fonts/`; Consolas lida do Windows.
- A DLL nova não tem eject: para trocar de build, reabrir o jogo.
- Observação visual: o jogo aparece mais através do painel do que a
  opacidade de 93% sugere; reavaliar quando houver tabelas.

## Sessão 8 — Story 1.3: estado do Scout por save (2026-10-01)

- Cada carreira tem o próprio JSON em
  `%LOCALAPPDATA%\FifaCompanion\scout\<hash>.json` (hash = SHA-256 da
  identidade da Story 1.1). Formato: `versao`, `olheiros`, `missoes`,
  `relatorios`, `ui_prefs.aba_ativa` (`"olheiros"`/`"missoes"`/
  `"relatorios"`/`"sonar"`).
- Gravação write-through (`.tmp` + rename) por um único mutex por
  arquivo (`scout::persistence::EstadoPersistido`); o `scout::state`
  guarda um estado por hash durante a sessão inteira.
- Arquivo corrompido vira `<hash>.json.corrompido-<ms>` antes de começar
  vazio; arquivo ilegível ou de formato mais novo nunca é sobrescrito.
- A aba ativa volta ao reabrir a mesma carreira (depois da localização
  de ~15 s). Clique em aba sem carreira pronta não é salvo.
- Para inspecionar/zerar o estado de uma carreira: apagar o arquivo dela
  com o jogo fechado (o log mostra os 8 primeiros caracteres do hash em
  `[scout::state] Carreira ativa: estado xxxxxxxx…`).

## Sessão 9 — Story 1.7: início com o jogo e carreira pré-carregada (2026-10-01)

- `fifa_overlay\iniciar_fifa.ps1` (como Administrador; pede UAC sozinho)
  abre o servidor do FIFA Friends (`D:\Program Files\FIFA 16\Server16Python.exe`,
  alvo do atalho da área de trabalho) e deixa `fifa_injector --aguardar
  --enquanto-pid <servidor>` esperando o `fifa16.exe`. Ele injeta quando o
  jogo tem `d3d11.dll` e janela visível, depois de 5 s de folga.
- Vigia no `scout::state`: com o painel fechado, a cada 2 s um `AsyncTask`
  lê 3 × 4 bytes por região (offsets da data viva). Se 2 de 3 concordam, há
  carreira, e ele localiza sozinho.
- **Localização rápida confirmada no jogo**: varrendo só as regiões com data
  viva, achou a struct em **4 ms** (~110 ms com a leitura dos saves), contra
  ~15 s da varredura completa. A struct de finanças e a data viva ficam na
  MESMA região (`0x8CCB0000`, struct em +0xEE74C, data em +0x373E08).
- Banner no canto superior direito (3 s): "Central de Scout ativa",
  "Carregando carreira…", "Carreira pronta", falha.

## Sessão 10 — Stories 1.4 e 1.5: aba Olheiros e contratação (2026-10-01)

- Aba Olheiros: as 12 combinações Especialização × Tier, com custos em
  `scout::quality` (Júnior 0,3–0,5 M; Experiente 1,2–1,9 M; Elite 3,6–5,8 M).
- **Primeira escrita real do overlay, confirmada até o save**:
  `save_repo::write_transfer_budget(anterior, novo)` escreve os 4 bytes do
  `transferbudget` na struct viva (só se o valor ainda for `anterior`) e
  relê. Contratar um Caçador de Jovens Júnior: 45.955.973 → 45.555.973.
  Depois de salvar no jogo, o `DATA` novo abre normal (checksum refeito
  pelo jogo) e traz 45.555.973, com os outros 25 campos do `dqXv`
  idênticos ao save anterior. A segunda cópia do orçamento (sessão 6) não
  desfaz a escrita.
- Ideias do Felipe para Olheiros pós-v1:
  `_bmad-output/planning-artifacts/melhorias-futuras-olheiros.md`.

## Sessão 11 — Story 1.6: navegação por controle e recarga sem fechar o jogo (2026-10-01)

- O FIFA lê o controle direto pelo XInput: o `MessageFilter` do hudhook não
  o bloqueia. `gamepad.rs` engancha `XInputGetState` (MinHook do hudhook)
  nas DLLs de XInput carregadas (no jogo: DLL #0 `xinput1_4` e #2
  `xinput9_1_0`). Com o painel aberto, e até soltar os botões depois de
  fechar, o jogo recebe o controle parado. O overlay lê pelo trampolim.
- L3+START abre/fecha; LB/RB trocam de aba; B volta/fecha; cada card da
  aba Olheiros é um item navegável inteiro (o scroll mostra o card todo).
- **Recarga de desenvolvimento**: `fifa_overlay/recarregar_dev.ps1` (com o
  jogo aberto) cria `%TEMP%/fifa_overlay_eject.pedido`; a DLL (>= 1.6-v2)
  remove o gancho do XInput e chama `hudhook::eject()`; o script copia a
  build nova e injeta. Primeiro teste: ciclo completo em ~2 s, jogo seguiu.

## Sessão 12 — Épico 2 inteiro: busca, Relatórios, filtros, Missão contínua (2026-10-01)

Stories 2.4 a 2.10 implementadas em sequência, sem teste em jogo entre
elas (Felipe pediu "desenvolva todas do épico 2"). Branch
`claude/epico-2-restante` (um commit por story, em cima da 2.3);
build `2.10-v1`.

- **Jogadores vêm do `DATA` do save ativo no disco** (o arquivo que a
  localização escolheu pela memória), não da memória: o blob `CZUM` do
  heap é o buffer do último load/save, mesmo conteúdo. Leitura completa
  (39.229 jogadores, nomes, clube, nação) em < 0,5 s. Consequência: o
  Relatório reflete o último save do jogo.
- **Nomes:** `editedplayernames` (save, por playerid) → `commonnameid`
  ou primeiro+último nome, procurados em `dcplayernames` (save, regens,
  ids ≥ 29000) e depois em `playernames` (banco estático, Huffman). Ids
  presentes nas duas tabelas têm o mesmo texto. Clube via
  `teamplayerlinks` ignorando seleções (`teamnationlinks`); liga 76 =
  "Rest of World" (fora do mercado). `Crbb.confederation`: 2 Europa,
  3 África, 4 América do Sul, 5 Ásia, 6 Oceania, 7 América do Norte,
  1 "Rest of World".
- **Minifaces:** `data/ui/imgAssets/heads/p<id>.dds`, 128×128 DXT5
  (40.504 arquivos soltos no disco do Felipe); `notfound.dds` existe.
- **AD-8 emendado** (Story 2.10): a busca roda na primeira abertura do
  painel depois de criar/renovar a Missão; o Relatório é revelado aos
  poucos (`ceil(progresso × alvo)`); Missão contínua em blocos de 30
  dias, renovação só por compra confirmada.
- Clippy da toolchain nova (1.98) aponta lints novos em código antigo
  (`div_ceil`, `as_chunks`, `is_multiple_of`); o código novo está limpo.

## Sessão 13 — Épico 3 inteiro: Ficha, Radar, comparação, Fit Posicional, Jogador de Referência (2026-10-03)

Stories 3.1 a 3.5 implementadas de uma vez, em paralelo com o teste do
Épico 2 pelo Felipe (sem teste em jogo). Branch
`claude/desenvolvimento-paramo-489fb3`, build `3.5-v1`; 202 testes.

- **Pé preferido:** `CZUM.preferredfoot` (`MDvm`, rangelow 1: 1 = direito,
  2 = esquerdo). Canhotos são minoria no save, como esperado.
- **Elenco vem do `DATA`, não da memória:** `read_squad_players` filtra
  `read_all_players` pelo `mPrV.clubteamid` (~0,5 s). Por isso roda num
  `AsyncTask` (emenda do AD-4/AD-13), relido a cada abertura do painel.
- **AD-6 emendado:** até 3 telas satélite (Relatório → Ficha → seletor).
- **Fit Posicional:** 11 perfis ideais com pesos que somam 100
  (`quality::PERFIS`); força = nota no perfil-alvo ÷ nota no perfil da
  posição nativa (teto 100%). Limiar 95: com 92 metade da base passava
  (posições vizinhas têm perfis parecidos).
- **Similaridade:** `0,75 × forma + 0,25 × nível` (forma = diferença média
  descontada a média de cada um). Limiar 75: passam de ~30 a ~3.300
  jogadores por referência no save do Felipe; o mais parecido fica em
  81–92%. Zagueiros têm perfis muito homogêneos.
- **Calibração reproduzível:** `cargo test --release calibracao --
  --ignored --nocapture` imprime quantos passam em cada limiar.
- O filtro usa os valores reais; o que o Relatório mostra (similaridade,
  fit) é recalculado só com as faixas reveladas e leva "≈" abaixo da
  Qualidade Alta.

### Ajustes pós-teste (mesma sessão, build `3.6-v1`)

Pedidos do Felipe depois de testar: filtro geográfico por **onde o
jogador joga** (continente → país → liga, numa tela só), Nova Missão
começando pelo Olheiro (com filtros ideais por Especialização), aba
Olheiros com Cards/Tabular, filtros de idade e de contrato, atributos
dominantes múltiplos. Ver a Story 3.6.

- **Ligas no save do FIFA Friends:** 79 em `onMQ`; "países" especiais no
  `countryid`: 75 seleções, 210 passes livres, 211 "Rest of World" (ligas
  "Clubes da UEFA/Concacaf/AFC/CAF-OFC", sem país), 216 creation zone,
  156/217/220/221/223/224 = federações estaduais brasileiras (valem como
  Brasil). Nenhum time está em duas ligas.
- **Textos em Latin-1:** nomes de times e ligas do banco do FIFA Friends
  não são UTF-8 ("São Caetano" aparecia "S�o Caetano").
- **Contrato:** `CZUM.contractvaliduntil` (`qvmK`) é o ano; no save, quase
  tudo entre 2036 e 2040 (carreira em 2035).

## Próximos passos sugeridos (não implementados)

Em ordem aproximada de valor/esforço. **Atualizado após sessão 3** —
ver seção "Exploração de escrita em memória (sessão 3)" acima antes de
retomar qualquer item de escrita, para não repetir becos sem saída já
mapeados.

1. **Simulador de treinamento OFFLINE, sem escrita automática (caminho
   pragmático recomendado)**: construir a lógica estatística de
   simulação de treinamento operando sobre os dados já lidos do save
   (via `fifa16_search.py`/`fifa16_db_parser.py`), gerando como saída
   uma lista clara de "jogador X: atributo Y de A para B" para
   múltiplos jogadores de uma vez. A aplicação real de cada mudança
   fica manual (usuário abre cada jogador na tela de edição do FIFA,
   se existir, e digita o valor calculado) ou é adiada até resolver
   automação de UI (item 2). Evita totalmente os dois becos sem saída
   da sessão 3 (checksum do arquivo, blob de memória não-vivo) e
   entrega valor imediato (a parte difícil — a lógica de simulação —
   fica pronta e testável).
2. **Automação de UI + escrita via memória por jogador**: dado que a
   única técnica de escrita comprovadamente funcional é a cadeia de
   ponteiros por jogador (estilo Cheat Table/Live Editor, que exige a
   tela de edição do jogador aberta), avaliar se um script pode
   automatizar a navegação (via simulação de input de
   teclado/controle) para abrir cada jogador, aplicar os valores
   calculados pelo simulador do item 1, e fechar — repetindo em lote.
   Complexidade alta (depende de UI automation confiável, sujeito a
   quebrar com qualquer mudança de layout de menu) e ainda não
   validamos se a tela de EDIÇÃO (não só "Perfil") existe de fato no
   fluxo do FIFA 16 do usuário — isso precisa ser confirmado antes de
   investir nesse item.
3. **NÃO recomendado a esta altura: decifrar o checksum do arquivo
   DATA**. Já foi tentado exaustivamente na sessão 3 (força bruta
   em todas as janelas possíveis do cabeçalho × múltiplas variantes de
   CRC32 × 15 saves reais simultâneos — zero matches) e é bloqueado
   por um packer/anti-tamper real no executável (seções PE com layout
   inconsistente, ver sessão 3). A comunidade (xAranaktu, ~6 anos
   mantendo ferramentas equivalentes para FIFA 19–FC 26) nunca resolveu
   isso por essa via — só reforça que não vale insistir sem
   engenharia reversa dinâmica de alto risco (fora de escopo).
4. **NÃO recomendado: reescrever o blob de database completo em
   memória (`CZUM` no heap)**. Confirmado na sessão 3 que a escrita
   funciona mas não tem nenhum efeito observável no jogo (não é a
   fonte "viva" consultada pela UI/lógica) — não retomar sem uma nova
   hipótese de por que esse blob existiria e uma forma de confirmar
   que é consultado por algo.
5. **Mapear mais tabelas** (times, táticas, negociações de
   transferência mais a fundo) seguindo o mesmo processo usado para
   `CZUM`/`BGwe`/`Crbb`: decodificar sem metadata, achar candidatos
   por heurística (contagem de linhas, tipos de campo), validar com
   dados conhecidos. Útil independente do caminho de escrita escolhido,
   já que o simulador do item 1 precisa ler mais contexto (ex: minutos
   jogados, posição, categoria de base) para calcular o treinamento.
6. **Melhorar a UX do atalho**: resolver o problema de "apertar duas
   vezes" durante transição de fullscreen (ver seção "Por que às
   vezes preciso apertar o atalho duas vezes?" acima), ou migrar
   para modo Borderless/Fullscreen Windowed do FIFA se o jogo suportar
   (eliminaria o problema de raiz sem mudar código).
7. **Resolver paddles do 8BitDo**: investigar app/firmware do
   controle para mapear paddles a um botão livre, permitindo um combo
   mais ergonômico que LEFT_THUMB+START.
8. **Retomar M1** (detecção automática de menu) via DLL injection +
   hook de `Present()`/função de transição de tela, se o overlay real
   se tornar prioridade. Escopo grande (C++/Rust), ver notas de M1.
9. **Empacotar o app** (`electron-builder` ou similar) para não
   precisar rodar `npm start` manualmente — gerar um `.exe` que já
   inicia o gamepad watcher junto.
