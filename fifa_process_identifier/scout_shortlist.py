"""
Acrescenta um jogador à Lista de Escolhidos nativa do FIFA 16 (memória viva),
para o experimento. Terminal elevado, jogo aberto, carreira carregada.

A lista é um vetor (ponteiros início/fim/fim-da-capacidade de 8 bytes numa
estrutura dona) de entradas de 28 bytes: `team, playerid, -1, -1, -1, -1, 1`
(7 int32; os 4 `-1` ficam como o jogo grava quando nada foi revelado). O dono é
achado como a localização do ponteiro de início
(`scout_probe.py capture X --int <ponteiro_de_início_com_sinal>`).

    python scout_shortlist.py 0x8CC85A68 --list
    python scout_shortlist.py 0x8CC85A68 --add --team 1808 --player 73885
    python scout_shortlist.py 0x8CC85A68 --remove 73885

Conferências: vetor coerente, entradas no formato, cabe na capacidade, sem
duplicata, conteúdo estável entre leitura e escrita; relê no fim. O ponteiro
de fim só avança DEPOIS de a entrada estar escrita.
"""
from __future__ import annotations

import argparse
import struct
import sys

import process
from scout_insert import rd, wr

sys.stdout.reconfigure(encoding="utf-8")
ENT = 28


def load(handle, owner: int):
    begin, end, cap = struct.unpack("<QQQ", rd(handle, owner, 24))
    if not (begin < end <= cap) or (end - begin) % ENT or begin % 4 or end - begin > 120 * ENT:
        sys.exit(f"[RECUSADO] vetor incoerente: begin=0x{begin:X} end=0x{end:X} cap=0x{cap:X}")
    n = (end - begin) // ENT
    raw = rd(handle, begin, n * ENT)
    ents = [struct.unpack_from("<7i", raw, i * ENT) for i in range(n)]
    for e in ents:
        if not (0 < e[1] < 400000 and e[2:6] != (0, 0, 0, 0) and (e[6] & 0xFF) in (0, 1)):
            sys.exit(f"[RECUSADO] entrada fora do formato: {e}")
    return begin, end, cap, ents, raw


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("owner")
    ap.add_argument("--list", action="store_true")
    ap.add_argument("--add", action="store_true")
    ap.add_argument("--team", type=int)
    ap.add_argument("--player", type=int)
    ap.add_argument("--remove", type=int, help="playerid a remover (sem lacuna: compacta)")
    args = ap.parse_args()

    pid = process.find_process_by_name("FIFA16.exe")
    if pid is None:
        sys.exit("[ERRO] FIFA16.exe não está rodando.")
    handle = process.open_process_write(pid)
    owner = int(args.owner, 16)
    begin, end, cap, ents, raw = load(handle, owner)
    print(f"lista: begin=0x{begin:X} end=0x{end:X} cap=0x{cap:X}  entradas={len(ents)}  capacidade={(cap - begin) // ENT}")
    for e in ents:
        print("  ", e[:6], "flag", e[6] & 0xFF)

    if args.add:
        if args.team is None or args.player is None:
            sys.exit("[ERRO] --add precisa de --team e --player")
        if any(e[1] == args.player for e in ents):
            sys.exit("[RECUSADO] jogador já está na lista")
        if end + ENT > cap:
            sys.exit("[RECUSADO] lista cheia")
        if rd(handle, begin, len(raw)) != raw:
            sys.exit("[RECUSADO] a lista mudou durante a leitura; tente de novo")
        wr(handle, end, struct.pack("<7i", args.team, args.player, -1, -1, -1, -1, 1))
        wr(handle, owner + 8, struct.pack("<Q", end + ENT))
    elif args.remove is not None:
        keep = [e for e in ents if e[1] != args.remove]
        if len(keep) == len(ents):
            sys.exit("[RECUSADO] jogador não está na lista")
        if rd(handle, begin, len(raw)) != raw:
            sys.exit("[RECUSADO] a lista mudou durante a leitura; tente de novo")
        wr(handle, owner + 8, struct.pack("<Q", begin + len(keep) * ENT))  # encolhe primeiro
        wr(handle, begin, b"".join(struct.pack("<7i", *e) for e in keep))
    else:
        return

    _, _, _, ents2, _ = load(handle, owner)
    print(f"depois: entradas={len(ents2)}")
    for e in ents2:
        print("  ", e[:6], "flag", e[6] & 0xFF)


if __name__ == "__main__":
    main()
