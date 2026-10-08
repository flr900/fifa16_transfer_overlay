"""
Ferramenta de análise OFFLINE da imagem despejada (`%TEMP%\fifa16_imagem.bin`), usada em 2026-10-08.
Não toca no jogo. A faixa de código real desta build é RVA 0x39FC000..0x9514000 (seção "`.tls em`").
Ver `_bmad-output/planning-artifacts/integracao-negociacao.md`.
"""
import sys, mmap, os
from capstone import Cs, CS_ARCH_X86, CS_MODE_64
tmp = os.environ['TEMP']
f = open(os.path.join(tmp, 'fifa16_imagem.bin'), 'rb')
mm = mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ)
BASE = 0x140000000
rva = int(sys.argv[1], 16); n = int(sys.argv[2]) if len(sys.argv) > 2 else 256
ini = rva - (int(sys.argv[3]) if len(sys.argv) > 3 else 0)
md = Cs(CS_ARCH_X86, CS_MODE_64)
md.detail = False
def texto_em(r):
    b = mm[r:r+80]
    z = b.find(b'\0')
    t = b[:z if z >= 0 else 80]
    return t.decode('latin1') if len(t) >= 4 and all(32 <= c < 127 for c in t) else None
for i in md.disasm(mm[ini:ini+n], BASE + ini):
    extra = ''
    if i.mnemonic == 'lea' and 'rip' in i.op_str:
        # alvo = endereço + tamanho + disp
        import re
        m = re.search(r'rip ([+-]) (0x[0-9a-f]+)', i.op_str)
        if m:
            d = int(m.group(2), 16) * (1 if m.group(1) == '+' else -1)
            alvo = i.address + i.size + d - BASE
            t = texto_em(alvo) if 0 < alvo < len(mm) else None
            extra = f'   ; -> 0x{alvo:X}' + (f' "{t}"' if t else '')
    if i.mnemonic == 'call':
        extra = '   ; RVA 0x%X' % (int(i.op_str, 16) - BASE) if i.op_str.startswith('0x') else ''
    print(f'{i.address - BASE:08X}  {i.mnemonic:<6} {i.op_str}{extra}')
