# fifa_overlay_debug — janela de diagnóstico (arquivada)

Cópia do crate `fifa_overlay` como estava ao fim da Story 1.1 (build
`s6-v10`, 2026-09-30), guardada quando o overlay principal passou a
ter só a Central de Scout (Story 1.2).

Contém a janela "FIFA 16 Companion" com as ferramentas usadas para
descobrir onde o jogo guarda os dados vivos:

- **Localizar carreira / Reler valores**: mesmo `save_repo` da Story 1.1.
- **Sonda de estado vivo**: procura um valor âncora e os valores vizinhos.
- **Scan por valor exato (i32)** + próximo scan: registra endereços e a
  vizinhança no log quando sobram ≤ 50.
- Pointer scan reverso, scan do blob `CZUM` e teste de escrita (PoC
  das sessões 3–5).

Não é mantida junto com o `fifa_overlay`. O `save_repo` daqui é uma
cópia congelada. Use para investigação de memória e não como base de
features.

## Uso

```powershell
cargo build --release
# FIFA aberto, PowerShell como Administrador:
..\fifa_injector\target\release\fifa_injector.exe target\release\fifa_overlay_debug.dll
```

Log: `%TEMP%\fifa_overlay_debug.log`. Não injete junto com o
`fifa_overlay`, porque os dois instalam o mesmo hook de `Present()`.
