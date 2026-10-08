"""
Ferramenta de análise OFFLINE da imagem despejada (`%TEMP%\fifa16_imagem.bin`), usada em 2026-10-08.
Não toca no jogo. A faixa de código real desta build é RVA 0x39FC000..0x9514000 (seção "`.tls em`").
Ver `_bmad-output/planning-artifacts/integracao-negociacao.md`.
"""
import os, sys, mmap
import numpy as np
tmp = os.environ['TEMP']
f = open(os.path.join(tmp, 'fifa16_imagem.bin'), 'rb')
mm = mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ)
INI, FIM = 0x39FC000, 0x39FC000 + 0x5B18000
code = np.frombuffer(mm[INI:FIM], dtype=np.uint8)
alvos = [int(x, 16) for x in sys.argv[1:]]
achados = []
for k in range(4):
    n = (len(code) - k) // 4
    arr = np.frombuffer(code[k:k + n * 4].tobytes(), dtype='<i4')
    pos = k + 4 * np.arange(n, dtype=np.int64)
    for alvo in alvos:
        for imm in (0, 1, 4):   # bytes depois do disp32 na instrução (mov [rip+x], imm8/imm32)
            esperado = alvo - (INI + pos + 4 + imm)
            m = np.nonzero(arr == esperado.astype(np.int32))[0]
            for q in m:
                achados.append((INI + int(pos[q]), alvo, imm))
for rva, alvo, imm in sorted(set(achados))[:60]:
    print(f'disp32 em 0x{rva:X} (instrução ~0x{rva-3:X}) -> 0x{alvo:X} (imm {imm})')
print(len(set(achados)), 'ocorrências')
