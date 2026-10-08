//! Roteiro "Abrir no jogo" (2026-10-08): leva o FIFA até o menu de um jogador
//! da lista de Escolhidos do jogo, apertando os botões pelo mesmo gancho do
//! XInput que já esconde o controle do jogo.
//!
//! Máquina de estados pura: a cada frame recebe o evento de tela que o jogo
//! mostra (`telas::Evento`), o jogador em foco e o tempo, e devolve os botões
//! a apertar. Nada aqui toca o jogo; o encaixe fica em `Scout::frame`.
//!
//! O roteiro NÃO mexe no hub: lá está o bloco "Avançar", e um `A` no bloco
//! errado avançaria o calendário. Ele começa com a lista de Escolhidos já
//! aberta (o jogador entra nela) e a varre:
//!
//! 1. `A` na linha do cursor; espera o menu do jogador (`ActionPopup`);
//! 2. espera o jogo mostrar quem é (o foco atualiza ~0,3 s depois do menu);
//! 3. se for o alvo, para com o menu aberto (a escolha da opção é de quem joga);
//! 4. se não, `B` (volta à lista), espera a lista e `↓` para a próxima linha.
//!
//! Cada espera tem prazo: um evento que não chega, ou chega fora de hora,
//! encerra o roteiro com um motivo, em vez de seguir apertando às cegas.

use crate::gamepad::botao;
use crate::telas::Evento;

/// Quanto o botão fica apertado e quanto se espera depois de soltar.
pub const APERTO_MS: u64 = 80;
pub const PAUSA_MS: u64 = 140;
/// Prazos de espera de cada evento.
const PRAZO_MENU_MS: u64 = 2_000;
const PRAZO_LISTA_MS: u64 = 3_000;
const PRAZO_INICIO_MS: u64 = 2_000;
/// O foco do jogo atualiza ~0,3 a 0,4 s depois do menu abrir (gravações).
const ESPERA_FOCO_MS: u64 = 800;
/// Depois do B a lista avisa que carregou (`NotifyScreenLoadedAndRefresh`) mas
/// ainda se atualiza por ~1,5 s (o aviso vem duas vezes nas gravações) e
/// ignora botões nesse tempo: o primeiro teste no jogo (7.6-v30) perdeu o `A`.
const ASSENTAR_LISTA_MS: u64 = 1_300;
/// Depois do `↓`, o cursor precisa assentar antes do `A`.
const ASSENTAR_BAIXO_MS: u64 = 350;
/// Quanto esperar, depois disso, por uma leitura do jogador em foco.
const PRAZO_FOCO_MS: u64 = 1_500;
/// Tempo máximo de um roteiro inteiro.
const PRAZO_TOTAL_MS: u64 = 600_000;

/// O que aconteceu no fim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resultado {
    /// O menu do jogador procurado está aberto no jogo.
    Encontrou,
    NaoEstaNaLista,
    /// Varreu a lista inteira sem achar o jogador.
    NaoEncontrou,
    Cancelou,
    Falhou(String),
}

impl Resultado {
    /// A frase para o jogador (sem exclamação).
    pub fn texto(&self, nome: &str) -> String {
        match self {
            Resultado::Encontrou => format!("{nome} está aberto no jogo."),
            Resultado::NaoEstaNaLista => {
                "Abra a lista de Escolhidos do jogo (pelo hub) e peça de novo: a Central só navega de dentro dela.".to_string()
            }
            Resultado::NaoEncontrou => format!("{nome} não está na lista de Escolhidos do jogo."),
            Resultado::Cancelou => "Navegação cancelada.".to_string(),
            Resultado::Falhou(motivo) => format!("A navegação parou: {motivo}"),
        }
    }
}

/// O que o roteiro vê a cada frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entrada {
    /// Milissegundos desde que o roteiro começou.
    pub agora_ms: u64,
    /// O evento que o jogo mostra agora (`None` = sem leitura).
    pub evento: Option<Evento>,
    /// A lista de Escolhidos do jogo está aberta.
    pub na_lista: bool,
    /// O jogador que o jogo mostra em foco (id).
    pub foco: Option<u32>,
    /// O jogador pediu para parar.
    pub cancelou: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saida {
    /// Botões a apertar neste frame (máscara do XInput).
    pub botoes: u16,
    pub fim: Option<Resultado>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fase {
    Comecar,
    ApertarA,
    EsperarMenu,
    EsperarFoco,
    EsperarLista,
    ApertarBaixo,
    AssentarBaixo,
    Fim,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Toque {
    mascara: u16,
    solta_em_ms: u64,
}

#[derive(Debug, Clone)]
pub struct Roteiro {
    alvo: u32,
    fase: Fase,
    /// Quando a fase começou.
    desde_ms: u64,
    toque: Option<Toque>,
    /// Depois do toque e da pausa, vai para esta fase.
    apos: Option<Fase>,
    espera_ate_ms: u64,
    sondas: u32,
    limite_de_sondas: u32,
    /// Vem da lista do jogo: a segunda vez que a mesma linha aparece, a
    /// lista não deu a volta como se esperava.
    ultimo_visto: Option<u32>,
    /// O resultado que vale depois de um último toque (fechar o menu antes de desistir).
    final_depois_do_toque: Option<Resultado>,
    fim: Option<Resultado>,
}

impl Roteiro {
    /// `tamanho_da_lista`: quantos jogadores a lista do jogo tem (limita a varredura).
    pub fn new(alvo: u32, tamanho_da_lista: usize) -> Self {
        Roteiro {
            alvo,
            fase: Fase::Comecar,
            desde_ms: 0,
            toque: None,
            apos: None,
            espera_ate_ms: 0,
            sondas: 0,
            limite_de_sondas: u32::try_from(tamanho_da_lista).unwrap_or(u32::MAX).saturating_add(2),
            ultimo_visto: None,
            final_depois_do_toque: None,
            fim: None,
        }
    }

    pub fn sondas(&self) -> u32 {
        self.sondas
    }

    fn terminar(&mut self, resultado: Resultado) -> Saida {
        self.fase = Fase::Fim;
        self.toque = None;
        self.fim = Some(resultado.clone());
        Saida { botoes: 0, fim: Some(resultado) }
    }

    /// Aperta `mascara` por `APERTO_MS`, solta, espera `PAUSA_MS` e vai para `proxima`.
    fn tocar(&mut self, mascara: u16, agora: u64, proxima: Fase) -> Saida {
        self.toque = Some(Toque { mascara, solta_em_ms: agora + APERTO_MS });
        self.apos = Some(proxima);
        Saida { botoes: mascara, fim: None }
    }

    fn ir_para(&mut self, fase: Fase, agora: u64) {
        self.fase = fase;
        self.desde_ms = agora;
    }

    pub fn passo(&mut self, e: &Entrada) -> Saida {
        if let Some(fim) = &self.fim {
            return Saida { botoes: 0, fim: Some(fim.clone()) };
        }
        if e.cancelou {
            return self.terminar(Resultado::Cancelou);
        }
        if e.agora_ms > PRAZO_TOTAL_MS {
            return self.terminar(Resultado::Falhou("o tempo acabou".to_string()));
        }
        if let Some(t) = self.toque {
            if e.agora_ms < t.solta_em_ms {
                return Saida { botoes: t.mascara, fim: None };
            }
            self.toque = None;
            self.espera_ate_ms = e.agora_ms + PAUSA_MS;
        }
        if e.agora_ms < self.espera_ate_ms {
            return Saida { botoes: 0, fim: None };
        }
        if let Some(proxima) = self.apos.take() {
            self.ir_para(proxima, e.agora_ms);
        }
        let decorrido = e.agora_ms.saturating_sub(self.desde_ms);
        match self.fase {
            Fase::Comecar => match e.evento {
                // já está no menu do jogador procurado: nada a fazer
                Some(Evento::Menu) if e.foco == Some(self.alvo) => self.terminar(Resultado::Encontrou),
                // o menu de outro jogador: fecha, e a linha dele não é o alvo
                Some(Evento::Menu) => {
                    self.ultimo_visto = e.foco;
                    self.sondas += 1;
                    self.tocar(botao::B, e.agora_ms, Fase::EsperarLista)
                }
                Some(Evento::TelaCarregada) if e.na_lista => {
                    self.ir_para(Fase::ApertarA, e.agora_ms);
                    self.passo(e)
                }
                _ if decorrido > PRAZO_INICIO_MS => self.terminar(Resultado::NaoEstaNaLista),
                _ => Saida { botoes: 0, fim: None },
            },
            // o A só vai com a lista na tela: em outra tela ele faria outra coisa
            Fase::ApertarA => match e.evento {
                Some(Evento::TelaCarregada) => self.tocar(botao::A, e.agora_ms, Fase::EsperarMenu),
                Some(Evento::Hub) => self.terminar(Resultado::Falhou("o jogo voltou ao hub".to_string())),
                _ if decorrido > PRAZO_LISTA_MS => self.terminar(Resultado::Falhou("a lista não está na tela".to_string())),
                _ => Saida { botoes: 0, fim: None },
            },
            Fase::EsperarMenu => match e.evento {
                Some(Evento::Menu) => {
                    self.ir_para(Fase::EsperarFoco, e.agora_ms);
                    Saida { botoes: 0, fim: None }
                }
                Some(Evento::Contrato | Evento::Compra) => {
                    self.terminar(Resultado::Falhou("abriu uma negociação em vez do menu do jogador".to_string()))
                }
                Some(Evento::Hub) => self.terminar(Resultado::Falhou("o jogo voltou ao hub".to_string())),
                _ if decorrido > PRAZO_MENU_MS => self.terminar(Resultado::Falhou("o menu do jogador não abriu".to_string())),
                _ => Saida { botoes: 0, fim: None },
            },
            Fase::EsperarFoco => {
                if matches!(e.evento, Some(Evento::Hub)) {
                    return self.terminar(Resultado::Falhou("o jogo voltou ao hub".to_string()));
                }
                if decorrido < ESPERA_FOCO_MS {
                    return Saida { botoes: 0, fim: None };
                }
                if e.foco == Some(self.alvo) {
                    return self.terminar(Resultado::Encontrou);
                }
                // sem ler quem está em foco não dá para saber se é o alvo: não varre às cegas
                if e.foco.is_none() {
                    return if decorrido > ESPERA_FOCO_MS + PRAZO_FOCO_MS {
                        self.terminar(Resultado::Falhou("não consegui ler o jogador em foco no jogo".to_string()))
                    } else {
                        Saida { botoes: 0, fim: None }
                    };
                }
                self.sondas += 1;
                // a mesma linha duas vezes seguidas: o cursor não andou (fim da lista sem volta)
                let repetiu = e.foco.is_some() && e.foco == self.ultimo_visto;
                self.ultimo_visto = e.foco;
                if repetiu || self.sondas >= self.limite_de_sondas {
                    // fecha o menu (o B vale o tempo todo do aperto) e só então desiste
                    self.final_depois_do_toque = Some(Resultado::NaoEncontrou);
                    return self.tocar(botao::B, e.agora_ms, Fase::Fim);
                }
                self.tocar(botao::B, e.agora_ms, Fase::EsperarLista)
            }
            Fase::EsperarLista => match e.evento {
                // a lista avisa que carregou, mas só aceita botões depois de assentar
                Some(Evento::TelaCarregada) if decorrido >= ASSENTAR_LISTA_MS => {
                    self.ir_para(Fase::ApertarBaixo, e.agora_ms);
                    self.passo(e)
                }
                Some(Evento::TelaCarregada) => Saida { botoes: 0, fim: None },
                Some(Evento::Hub) => self.terminar(Resultado::Falhou("o jogo voltou ao hub".to_string())),
                _ if decorrido > PRAZO_LISTA_MS + ASSENTAR_LISTA_MS => self.terminar(Resultado::Falhou("a lista não voltou".to_string())),
                _ => Saida { botoes: 0, fim: None },
            },
            Fase::ApertarBaixo => self.tocar(botao::DPAD_BAIXO, e.agora_ms, Fase::AssentarBaixo),
            Fase::AssentarBaixo => {
                if decorrido >= ASSENTAR_BAIXO_MS {
                    self.ir_para(Fase::ApertarA, e.agora_ms);
                    self.passo(e)
                } else {
                    Saida { botoes: 0, fim: None }
                }
            }
            Fase::Fim => match self.final_depois_do_toque.take() {
                Some(resultado) => self.terminar(resultado),
                None => Saida { botoes: 0, fim: self.fim.clone() },
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Um jogo falso: lista de jogadores, cursor, e os eventos que ele
    /// mostra (com a defasagem de ~100 ms e a do foco de ~350 ms).
    struct JogoFalso {
        lista: Vec<u32>,
        cursor: usize,
        evento: Option<Evento>,
        na_lista: bool,
        foco: Option<u32>,
        /// (quando, o que acontece) pendentes.
        agenda: Vec<(u64, Acao)>,
        botoes_anteriores: u16,
        a_cada_botao: Vec<u16>,
        com_volta: bool,
        /// A lista ignora botões até este instante (como a de verdade, logo depois do B).
        ignora_ate: u64,
        /// Quanto a lista demora para voltar a aceitar botões depois do B.
        assenta_em: u64,
        ignorados: u32,
    }

    #[derive(Debug, Clone, Copy)]
    enum Acao {
        Evento(Evento),
        Foco(u32),
    }

    impl JogoFalso {
        fn novo(lista: &[u32], cursor: usize) -> Self {
            JogoFalso {
                lista: lista.to_vec(),
                cursor,
                evento: Some(Evento::TelaCarregada),
                na_lista: true,
                foco: Some(999),
                agenda: Vec::new(),
                botoes_anteriores: 0,
                a_cada_botao: Vec::new(),
                com_volta: true,
                ignora_ate: 0,
                assenta_em: 1_200,
                ignorados: 0,
            }
        }

        fn atualizar(&mut self, agora: u64, botoes: u16) {
            let mut novos = botoes & !self.botoes_anteriores;
            self.botoes_anteriores = botoes;
            if novos != 0 && novos != botao::B && agora < self.ignora_ate {
                // a lista ainda se atualiza: o botão se perde
                self.ignorados += 1;
                novos = 0;
            }
            if novos != 0 {
                self.a_cada_botao.push(novos);
            }
            if novos & botao::DPAD_BAIXO != 0 && self.evento == Some(Evento::TelaCarregada) {
                let ultimo = self.lista.len() - 1;
                self.cursor = if self.cursor == ultimo { if self.com_volta { 0 } else { ultimo } } else { self.cursor + 1 };
            }
            if novos & botao::A != 0 && self.evento == Some(Evento::TelaCarregada) {
                self.agenda.push((agora + 100, Acao::Evento(Evento::Menu)));
                self.agenda.push((agora + 100 + 350, Acao::Foco(self.lista[self.cursor])));
            }
            if novos & botao::B != 0 && self.evento == Some(Evento::Menu) {
                self.agenda.push((agora + 100, Acao::Evento(Evento::TelaCarregada)));
                self.ignora_ate = agora + self.assenta_em;
            }
            let (prontos, resto): (Vec<_>, Vec<_>) = self.agenda.drain(..).partition(|(t, _)| *t <= agora);
            self.agenda = resto;
            for (_, acao) in prontos {
                match acao {
                    Acao::Evento(e) => self.evento = Some(e),
                    Acao::Foco(id) => self.foco = Some(id),
                }
            }
        }
    }

    /// Roda o roteiro contra o jogo falso, um quadro a cada 16 ms.
    fn rodar(jogo: &mut JogoFalso, roteiro: &mut Roteiro, cancelar_em: Option<u64>) -> (Resultado, u64) {
        let mut botoes = 0;
        for quadro in 0..20_000u64 {
            let agora = quadro * 16;
            jogo.atualizar(agora, botoes);
            let saida = roteiro.passo(&Entrada {
                agora_ms: agora,
                evento: jogo.evento,
                na_lista: jogo.na_lista,
                foco: jogo.foco,
                cancelou: cancelar_em.is_some_and(|c| agora >= c),
            });
            botoes = saida.botoes;
            if let Some(fim) = saida.fim {
                jogo.atualizar(agora + 16, 0);
                return (fim, agora);
            }
        }
        panic!("o roteiro não terminou");
    }

    #[test]
    fn it_scans_down_the_list_until_the_target_is_open_and_leaves_the_menu_open() {
        let mut jogo = JogoFalso::novo(&[10, 20, 30, 40, 50], 0);
        let mut roteiro = Roteiro::new(40, 5);
        let (fim, _) = rodar(&mut jogo, &mut roteiro, None);
        assert_eq!(fim, Resultado::Encontrou);
        assert_eq!(jogo.cursor, 3);
        assert_eq!(jogo.evento, Some(Evento::Menu), "o menu do alvo fica aberto");
        assert_eq!(jogo.foco, Some(40));
        assert_eq!(roteiro.sondas(), 3, "10, 20 e 30 antes do 40");
        // A (10), B, ↓, A (20), B, ↓, A (30), B, ↓, A (40)
        assert_eq!(jogo.a_cada_botao.iter().filter(|b| **b == botao::DPAD_BAIXO).count(), 3);
        assert_eq!(jogo.a_cada_botao.last(), Some(&botao::A));
    }

    #[test]
    fn it_waits_for_the_list_to_settle_after_the_b_so_no_button_is_lost() {
        // a lista de verdade ignorou o A logo depois do B (teste no jogo, 7.6-v30)
        let mut jogo = JogoFalso::novo(&[10, 20, 30, 40], 0);
        let mut roteiro = Roteiro::new(30, 4);
        let (fim, _) = rodar(&mut jogo, &mut roteiro, None);
        assert_eq!(fim, Resultado::Encontrou);
        assert_eq!(jogo.ignorados, 0, "nenhum botão apertado com a lista ainda atualizando");
        assert_eq!(jogo.cursor, 2);
    }

    #[test]
    fn it_starts_from_wherever_the_cursor_is_and_wraps_around_the_end() {
        let mut jogo = JogoFalso::novo(&[10, 20, 30, 40, 50], 3);
        let mut roteiro = Roteiro::new(20, 5);
        let (fim, _) = rodar(&mut jogo, &mut roteiro, None);
        assert_eq!(fim, Resultado::Encontrou);
        assert_eq!(jogo.cursor, 1);
        assert_eq!(roteiro.sondas(), 3, "40, 50 e 10 antes do 20");
    }

    #[test]
    fn a_player_who_is_not_in_the_list_ends_after_one_full_pass_with_the_menu_closed() {
        let mut jogo = JogoFalso::novo(&[10, 20, 30], 0);
        let mut roteiro = Roteiro::new(77, 3);
        let (fim, _) = rodar(&mut jogo, &mut roteiro, None);
        assert_eq!(fim, Resultado::NaoEncontrou);
        assert_eq!(jogo.evento, Some(Evento::TelaCarregada), "voltou à lista, sem deixar o menu de outro jogador");
        assert!(roteiro.sondas() <= 5);
    }

    #[test]
    fn a_list_that_does_not_wrap_stops_at_the_repeated_row_instead_of_pressing_forever() {
        let mut jogo = JogoFalso::novo(&[10, 20, 30], 0);
        jogo.com_volta = false;
        let mut roteiro = Roteiro::new(77, 3);
        let (fim, _) = rodar(&mut jogo, &mut roteiro, None);
        assert_eq!(fim, Resultado::NaoEncontrou);
        assert_eq!(roteiro.sondas(), 4, "10, 20, 30 e o 30 de novo");
    }

    #[test]
    fn it_does_nothing_outside_the_list_and_never_presses_a_button() {
        let mut jogo = JogoFalso::novo(&[10, 20], 0);
        jogo.na_lista = false;
        jogo.evento = Some(Evento::Hub);
        let mut roteiro = Roteiro::new(10, 2);
        let (fim, _) = rodar(&mut jogo, &mut roteiro, None);
        assert_eq!(fim, Resultado::NaoEstaNaLista);
        assert!(jogo.a_cada_botao.is_empty(), "nenhum botão no hub: o bloco Avançar fica a um A de distância");
    }

    #[test]
    fn a_menu_already_open_on_the_target_finishes_at_once_and_on_another_player_it_is_closed_first() {
        let mut jogo = JogoFalso::novo(&[10, 20, 30], 1);
        jogo.evento = Some(Evento::Menu);
        jogo.foco = Some(20);
        let mut roteiro = Roteiro::new(20, 3);
        let (fim, _) = rodar(&mut jogo, &mut roteiro, None);
        assert_eq!(fim, Resultado::Encontrou);
        assert!(jogo.a_cada_botao.is_empty());

        let mut jogo = JogoFalso::novo(&[10, 20, 30], 0);
        jogo.evento = Some(Evento::Menu);
        jogo.foco = Some(10);
        let mut roteiro = Roteiro::new(30, 3);
        let (fim, _) = rodar(&mut jogo, &mut roteiro, None);
        assert_eq!(fim, Resultado::Encontrou);
        assert_eq!(jogo.a_cada_botao[0], botao::B, "primeiro fecha o menu do outro jogador");
        assert_eq!(jogo.cursor, 2);
    }

    #[test]
    fn cancelling_stops_immediately_and_releases_every_button() {
        let mut jogo = JogoFalso::novo(&[10, 20, 30, 40, 50, 60, 70, 80], 0);
        let mut roteiro = Roteiro::new(80, 8);
        let (fim, quando) = rodar(&mut jogo, &mut roteiro, Some(2_000));
        assert_eq!(fim, Resultado::Cancelou);
        assert!((2_000..2_100).contains(&quando), "{quando}");
        let seguinte = roteiro.passo(&Entrada { agora_ms: quando + 16, evento: None, na_lista: true, foco: None, cancelou: false });
        assert_eq!(seguinte.botoes, 0);
    }

    #[test]
    fn a_missing_event_ends_the_script_with_the_reason_instead_of_pressing_on() {
        // o menu nunca abre: depois do A não vem ActionPopup
        let mut jogo = JogoFalso::novo(&[10, 20], 0);
        let mut roteiro = Roteiro::new(20, 2);
        let mut botoes = 0;
        let mut resultado = None;
        for quadro in 0..2_000u64 {
            let agora = quadro * 16;
            jogo.botoes_anteriores = botoes; // o A "não funciona": o jogo não reage
            let saida = roteiro.passo(&Entrada { agora_ms: agora, evento: Some(Evento::TelaCarregada), na_lista: true, foco: Some(1), cancelou: false });
            botoes = saida.botoes;
            if saida.fim.is_some() {
                resultado = saida.fim;
                break;
            }
        }
        assert_eq!(resultado, Some(Resultado::Falhou("o menu do jogador não abriu".to_string())));
    }

    #[test]
    fn without_a_reading_of_the_focused_player_it_stops_instead_of_scanning_blind() {
        let mut roteiro = Roteiro::new(20, 5);
        let mut botoes_vistos = Vec::new();
        let mut resultado = None;
        let mut menu_desde = None;
        for quadro in 0..2_000u64 {
            let agora = quadro * 16;
            // o menu abre 100 ms depois do primeiro A; o foco nunca é lido
            let evento = match menu_desde {
                Some(t) if agora >= t => Evento::Menu,
                _ => Evento::TelaCarregada,
            };
            let saida = roteiro.passo(&Entrada { agora_ms: agora, evento: Some(evento), na_lista: true, foco: None, cancelou: false });
            if saida.botoes == botao::A && menu_desde.is_none() {
                menu_desde = Some(agora + 100);
            }
            if saida.botoes != 0 {
                botoes_vistos.push(saida.botoes);
            }
            if saida.fim.is_some() {
                resultado = saida.fim;
                break;
            }
        }
        assert_eq!(resultado, Some(Resultado::Falhou("não consegui ler o jogador em foco no jogo".to_string())));
        assert!(botoes_vistos.iter().all(|b| *b == botao::A), "só o A inicial, nada de B nem ↓: {botoes_vistos:?}");
        assert_eq!(roteiro.sondas(), 0);
    }

    #[test]
    fn the_a_waits_for_the_list_and_never_goes_out_on_another_screen() {
        let mut roteiro = Roteiro::new(20, 5);
        // começa na lista, mas o evento vira "carregando" antes do A
        let ent = |ms, evento| Entrada { agora_ms: ms, evento: Some(evento), na_lista: true, foco: None, cancelou: false };
        assert_eq!(roteiro.passo(&ent(0, Evento::TelaCarregada)).botoes, botao::A);
        // depois do A e da volta à fase de espera, uma tela estranha: sem botões até o prazo
        let mut r2 = Roteiro::new(20, 5);
        r2.passo(&ent(0, Evento::TelaCarregada));
        let mut visto = Vec::new();
        for ms in (16..3_000).step_by(16) {
            let s = r2.passo(&ent(ms, Evento::Transicao));
            visto.push(s.botoes);
            if s.fim.is_some() {
                assert_eq!(s.fim, Some(Resultado::Falhou("o menu do jogador não abriu".to_string())));
                break;
            }
        }
        assert!(visto.iter().skip(5).all(|b| *b == 0), "nenhum botão enquanto a tela não é a esperada");
    }

    #[test]
    fn a_button_is_held_for_a_short_time_and_followed_by_a_pause() {
        let mut roteiro = Roteiro::new(1, 3);
        let ent = |ms| Entrada { agora_ms: ms, evento: Some(Evento::TelaCarregada), na_lista: true, foco: None, cancelou: false };
        assert_eq!(roteiro.passo(&ent(0)).botoes, botao::A, "aperta já no primeiro quadro");
        assert_eq!(roteiro.passo(&ent(APERTO_MS - 1)).botoes, botao::A);
        assert_eq!(roteiro.passo(&ent(APERTO_MS)).botoes, 0, "solta");
        assert_eq!(roteiro.passo(&ent(APERTO_MS + PAUSA_MS - 1)).botoes, 0, "pausa");
    }

    #[test]
    fn the_messages_are_plain_and_name_the_player() {
        assert!(Resultado::Encontrou.texto("Mbappé").contains("Mbappé"));
        assert!(Resultado::NaoEstaNaLista.texto("Mbappé").contains("lista de Escolhidos do jogo"));
        assert!(Resultado::NaoEncontrou.texto("Mbappé").contains("Mbappé"));
        for r in [Resultado::Encontrou, Resultado::NaoEstaNaLista, Resultado::NaoEncontrou, Resultado::Cancelou, Resultado::Falhou("x".into())] {
            assert!(!r.texto("a").contains('!'));
        }
    }
}
