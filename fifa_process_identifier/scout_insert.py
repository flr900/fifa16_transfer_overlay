"""
Insere (ou atualiza) o registro de conhecimento de um jogador no array vivo do
scout nativo do FIFA 16, para o experimento. Terminal elevado, jogo aberto.

O array é um vetor de registros de 20 bytes `[playerid, a, b, aaaammdd, -1]`,
ORDENADO por playerid, com ponteiros (início, fim, fim da capacidade) de 8
bytes numa estrutura dona. Passe o endereço da estrutura dona (o que guarda o
ponteiro de início), achado com
`scout_probe.py capture X --int <ponteiro_de_inicio_com_sinal>`.

    python scout_insert.py 0x8D0110C8 --player 71532 --a 1048578 --b 140 --date 20260710
    python scout_insert.py 0x8D0110C8 --list

Conferências: início/fim/capacidade coerentes, registros ordenados e válidos,
cabe na capacidade, o conteúdo não mudou entre a leitura e a escrita; depois
da escrita relê tudo. Se o jogador já tem registro, só atualiza `a`/`b`/data.
"""
from __future__ import annotations

import argparse
import ctypes
import struct
import sys

import memory
import process
from scout_poke import WriteProcessMemory

sys.stdout.reconfigure(encoding="utf-8")
REC = 20


def rd(handle, addr: int, n: int) -> bytes:
    buf = ctypes.create_string_buffer(n)
    got = ctypes.c_size_t(0)
    ok = memory.ReadProcessMemory(handle, ctypes.c_void_p(addr), buf, n, ctypes.byref(got))
    if not ok or got.value != n:
        sys.exit(f"[ERRO] leitura falhou em 0x{addr:X} ({n} bytes)")
    return buf.raw


def wr(handle, addr: int, data: bytes) -> None:
    wrote = ctypes.c_size_t(0)
    ok = WriteProcessMemory(handle, ctypes.c_void_p(addr), data, len(data), ctypes.byref(wrote))
    if not ok or wrote.value != len(data):
        sys.exit(f"[ERRO] escrita falhou em 0x{addr:X} (erro {ctypes.get_last_error()})")


def load(handle, owner: int):
    begin, end, cap = struct.unpack("<QQQ", rd(handle, owner, 24))
    if not (begin < end <= cap) or (end - begin) % REC or begin % 4 or end - begin > 400 * REC:
        sys.exit(f"[RECUSADO] vetor incoerente: begin=0x{begin:X} end=0x{end:X} cap=0x{cap:X}")
    n = (end - begin) // REC
    raw = rd(handle, begin, n * REC)
    recs = [struct.unpack_from("<5i", raw, i * REC) for i in range(n)]
    ids = [r[0] for r in recs]
    if ids != sorted(ids) or len(set(ids)) != n or any(r[4] != -1 or not 20200101 < r[3] < 20991231 for r in recs):
        sys.exit("[RECUSADO] registros fora do formato esperado (ordem/data/-1)")
    return begin, end, cap, recs, raw


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("owner", help="endereço (hex) da estrutura dona do vetor (begin,end,cap de 8 bytes)")
    ap.add_argument("--list", action="store_true")
    ap.add_argument("--player", type=int)
    ap.add_argument("--a", type=int)
    ap.add_argument("--b", type=int)
    ap.add_argument("--date", type=int)
    args = ap.parse_args()

    pid = process.find_process_by_name("FIFA16.exe")
    if pid is None:
        sys.exit("[ERRO] FIFA16.exe não está rodando.")
    handle = process.open_process_write(pid)
    owner = int(args.owner, 16)
    begin, end, cap, recs, raw = load(handle, owner)
    print(f"vetor: begin=0x{begin:X} end=0x{end:X} cap=0x{cap:X}  registros={len(recs)}  capacidade={(cap - begin) // REC}")
    if args.list or args.player is None:
        for r in recs:
            print("  ", r)
        return
    if None in (args.a, args.b, args.date):
        sys.exit("[ERRO] informe --a, --b e --date")

    novo = (args.player, args.a, args.b, args.date, -1)
    ids = [r[0] for r in recs]
    if args.player in ids:
        k = ids.index(args.player)
        print(f"já existe: {recs[k]} -> atualizando para {novo}")
        if rd(handle, begin + k * REC, REC) != raw[k * REC : (k + 1) * REC]:
            sys.exit("[RECUSADO] o registro mudou durante a leitura")
        wr(handle, begin + k * REC, struct.pack("<5i", *novo))
    else:
        if end + REC > cap:
            sys.exit("[RECUSADO] sem capacidade para mais um registro")
        k = sum(1 for i in ids if i < args.player)
        tail = raw[k * REC :]
        nova_cauda = struct.pack("<5i", *novo) + tail
        # relê logo antes de escrever: se mudou, aborta
        if rd(handle, begin, len(raw)) != raw:
            sys.exit("[RECUSADO] o array mudou durante a leitura; tente de novo")
        print(f"inserindo {novo} na posição {k} (antes de {recs[k] if k < len(recs) else 'fim'})")
        wr(handle, begin + k * REC, nova_cauda)  # 1º os dados
        wr(handle, owner + 8, struct.pack("<Q", end + REC))  # 2º o ponteiro de fim

    begin2, end2, cap2, recs2, _ = load(handle, owner)
    print(f"depois: registros={len(recs2)}")
    achou = [r for r in recs2 if r[0] == args.player]
    print("registro:", achou)
    print("OK" if achou == [novo] else "[ALERTA] a releitura não bateu")


if __name__ == "__main__":
    main()
