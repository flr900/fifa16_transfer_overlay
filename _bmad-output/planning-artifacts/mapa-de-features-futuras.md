# Mapa de features futuras — Central de Scout

> Mapeamento de 2026-10-08, a pedido do Felipe. Fontes: `prd.md` (§5, §6.2),
> `epics.md`, `sprint-status.yaml`, `melhorias-futuras-olheiros.md`,
> `integracao-scout-nativo.md`, `PROJECT_MEMORY.md`, memórias do projeto,
> branches e histórico do `main` (até `8489128`). Só análise de documentos e
> do git; nada foi testado em jogo.

## 1. Mapeado e ainda não feito

| Item | Origem | Situação |
|---|---|---|
| **Story 7.5**: calibrar o nível `b` e o campo `a` do conhecimento por informação (contrato, atributos, valor, salário) | `epics.md` Épico 7; `integracao-scout-nativo.md` | Sem arquivo de história e sem entrada no `sprint-status`. Commits recentes (níveis 140-143, 144, 162+, 178+, campo `a`) parecem ter feito boa parte na prática, sem registro. |
| **Story 7.7**: e-mail nativo do GTN | Épico 7 | Opcional, a de menor prioridade. A mais cara (objeto e strings localizadas). Alternativa barata: mostrar o "e-mail" só no overlay. |
| **Núcleo de olheiros detalhistas**: 2ª etapa de aprofundamento de um jogador já achado | PRD §6.2 | **Feito em 2026-10-08 (Story 6.2, "Aprofundar agora")**, estendendo o Generalista, só a partir dos Escolhidos (decisão do Felipe). Testado em jogo pelo Felipe, funcionando. |
| **Conhecimento progressivo por região** entre Missões (estilo Football Manager) | PRD §5 e §6.2 | Não implementado. A Base do Scout cobre uma parte (novas Missões consultam a Base primeiro). |
| **Limite de slots de Olheiros** | PRD §6.2 | **Adiado a pedido do Felipe (2026-10-08)**: o impacto no equilíbrio ainda não está claro. Nada no código. |
| **Integração com a negociação do jogo**: abrir compra, empréstimo e oferta salarial a partir da Central, e gerenciar negociações | Pedido do Felipe, 2026-10-08 | **Em andamento.** Roteiro "Abrir no jogo" funciona (7.6-v31) mas é lento (~3 s por linha) e exige a lista de Escolhidos do jogo aberta. Caminho direto (sem botões) investigado e **pausado** (sem campo simples de ação pendente). Próximo: salto por ordenação por nome. Detalhes em `integracao-negociacao.md`. Vem antes do e-mail nativo (7.7). |
| **Salário exato do jogo** | `integracao-scout-nativo.md`; Story 7.6 | Só o valor de transferência é exato, e só do jogador em foco. O salário segue estimado. Resolver exige chamar funções do jogo (caminho D, engenharia reversa do `fifa16.exe` com packer). |
| **Pendências do experimento nativo** | `integracao-scout-nativo.md` §3b; Story 7.6 | Achar os ponteiros sem varrer 1,9 GB (20-25 s por varredura); saber se a entrada de Escolhidos persiste no save; evitar que o jogo reescreva o `b` no estágio de 3 dias; conferir se o offset da linha de valor se mantém depois de reabrir o jogo. |

## 2. Branches com trabalho fora do `main`

> **Resolvido em 2026-10-08:** `nitidez-fontes` foi mesclada no `main` (PR #22, build `7.6-v24`) e `relatorio-ficha` foi descartada (fica a Ficha atual do `main`). Falta só remover a branch antiga `claude/nitidez-fontes` e o worktree `agent-ae03c6952f847100f`. A tabela abaixo é o registro do que havia.

| Branch | Conteúdo | Observação |
|---|---|---|
| `claude/nitidez-fontes` | Texto mais nítido com autohinting do FreeType (build `2.2-v5`) | Não entrou no `main`; o `theme.rs` ainda cita o FreeType como "próximo passo". |
| `claude/relatorio-ficha` | Ficha própria (radar de 6 eixos estilo carta FIFA, atributos paginados) e visão Tabular | Não portadas no porte de 2026-10-03; decisão nunca reavaliada. O resto (valor, salário, contrato, observação em estágios) já foi portado. |
| `claude/recarregar-dev-espera-descarga` | `recarregar_dev.ps1` espera a DLL descarregar | Provavelmente superado por `008e2df` e `bf9008c` do `main`; candidato a descarte. |

## 3. Ideias antigas do lado "editor" (`PROJECT_MEMORY.md`, "Próximos passos")

Lista anterior ao Scout; parte está obsoleta.

- **Simulador de treinamento offline**, sem escrita automática (era o caminho pragmático recomendado).
- **Escrita de atributos por jogador**: automação de UI, RTTI/vtable scanning, dump desempacotado + Ghidra. Segue sendo o bloqueio.
- **Mapear mais tabelas** (times, táticas, negociações).
- **Atalho que exige dois toques** na transição de fullscreen; ou migrar para borderless.
- **Paddles do 8BitDo**.
- **M1**: detecção automática de menu via hook de `Present()`.
- **Não recomendados**: decifrar o checksum do `DATA`; reescrever o blob `CZUM`.
- **Já feito**: empacotar o app (instalador).

## 4. Documentação defasada

> **Resolvido em 2026-10-08:** `sprint-status.yaml`, `epics.md` e PRD foram atualizados (histórias em `done`, 7.5 registrada, 7.7 em backlog, Sonar removido, mercado semanal, demissão, seção "Delivered outside the story flow"). O texto abaixo é o diagnóstico original.

- **`sprint-status.yaml`**: ~25 histórias em `review` que nunca foram para `done`; 7-5 e 7-7 ausentes; retrospectivas `optional`.
- **No `main`, mas sem registro no BMAD**: Base do Scout, tela de Configurações, carrossel de Olheiros, bandeiras, mercado semanal, renovação automática de contrato com multa de rescisão, instalador, ordenação de tabela por gamepad.
- **Contradizem o `main`**:
  - O Sonar (Épico 4) foi removido, mas `epics.md` ainda o lista.
  - O mercado virou semanal; os documentos dizem mensal.
  - O PRD lista a demissão de Olheiros como fora de escopo, mas ela existe.

## 5. Ordem sugerida

1. ~~**Fechar a contabilidade**~~ (feito em 2026-10-08): formalizar a 7.5, mover as histórias de `review` para `done`, atualizar `epics.md` e PRD.
2. ~~**Decidir as branches**~~ (feito em 2026-10-08): `nitidez-fontes` (mesclar?) e `relatorio-ficha` (Ficha/Tabular: portar ou descartar?).
3. **Escolher a próxima feature de produto**: núcleo de olheiros detalhistas (v2), limite de slots ou e-mail nativo.
