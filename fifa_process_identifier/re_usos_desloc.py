"""
Ferramenta de análise OFFLINE da imagem despejada (`%TEMP%\fifa16_imagem.bin`), usada em 2026-10-08.
Não toca no jogo. A faixa de código real desta build é RVA 0x39FC000..0x9514000 (seção "`.tls em`").
Ver `_bmad-output/planning-artifacts/integracao-negociacao.md`.
"""
import re, os, sys, mmap, struct
tmp = os.environ['TEMP']
f = open(os.path.join(tmp, 'fifa16_imagem.bin'), 'rb')
mm = mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ)
# seção de código real
INI, FIM = 0x39FC000, 0x39FC000 + 0x5B18000
code = mm[INI:FIM]
def inicio_func(pos):
    i = pos
    while i > max(0, pos - 20000):
        if code[i-1] == 0xCC and code[i] != 0xCC and (INI + i) % 16 == 0:
            return i
        i -= 1
    return pos
desl = [int(x, 16) for x in sys.argv[1:]]
por_func = {}
for d in desl:
    pat = re.compile(rb'[\x48\x49\x4c\x4d\x40-\x4f]?[\x8b\x8d\x89\x03\x3b\x83\xff][\x80-\xbf]' + struct.pack('<I', d), re.S)
    for m in pat.finditer(code):
        p = m.start()
        fi = inicio_func(p)
        por_func.setdefault(fi, {}).setdefault(d, []).append(p)
# funções que usam todos os deslocamentos pedidos
for fi, usos in sorted(por_func.items()):
    if len(usos) >= len(desl) - 0 and all(d in usos for d in desl):
        print(f'função ~0x{INI+fi:X}: ' + ', '.join(f'0x{d:X} em 0x{INI+p:X}' for d, ps in usos.items() for p in ps[:2]))
print('total de funções com algum uso:', len(por_func))
