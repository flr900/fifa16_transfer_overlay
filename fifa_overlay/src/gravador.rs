//! Gravador de controle (2026-10-08, só para desenvolvimento).
//!
//! Para a Central abrir a tela de negociação do jogo é preciso saber, para
//! cada caminho (comprar, empréstimo, contrato), a sequência exata de botões
//! e o tempo entre eles. Em vez de adivinhar, o gravador registra no log
//! (`%TEMP%\\fifa_overlay.log`, prefixo `[gravador]`) cada aperto e cada
//! soltura do controle e cada vez que o jogador em foco no jogo muda.
//!
//! Liga e desliga sozinho: existe enquanto o arquivo
//! `%TEMP%\\fifa_gravar_controle.pedido` existir (checado a cada segundo).
//! Desligado, não faz nada além dessa checagem. Só LÊ o controle (o mesmo
//! estado que o overlay já lê); nunca escreve nada no jogo.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::gamepad::{botao, EstadoControle, LIMIAR_GATILHO};

const INTERVALO_CHECAGEM: Duration = Duration::from_secs(1);
/// Quanto o analógico esquerdo precisa passar para contar como um aperto
/// numa direção (os menus do FIFA também navegam por ele).
const LIMIAR_ANALOGICO: i16 = 16_000;

/// Os botões que o gravador nomeia, na ordem em que aparecem no log.
const NOMES: [(u16, &str); 14] = [
    (botao::DPAD_CIMA, "D-pad ↑"),
    (botao::DPAD_BAIXO, "D-pad ↓"),
    (botao::DPAD_ESQUERDA, "D-pad ←"),
    (botao::DPAD_DIREITA, "D-pad →"),
    (botao::A, "A"),
    (botao::B, "B"),
    (botao::X, "X"),
    (botao::Y, "Y"),
    (botao::LB, "LB"),
    (botao::RB, "RB"),
    (botao::START, "Start"),
    (botao::BACK, "Select"),
    (botao::L3, "L3"),
    (botao::R3, "R3"),
];

/// Tudo o que conta como "apertado" num instante: botões, gatilhos e as
/// quatro direções do analógico esquerdo.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Apertos {
    botoes: u16,
    lt: bool,
    rt: bool,
    analogico_cima: bool,
    analogico_baixo: bool,
    analogico_esquerda: bool,
    analogico_direita: bool,
}

impl Apertos {
    fn de(estado: &EstadoControle) -> Self {
        Apertos {
            botoes: estado.botoes,
            lt: estado.lt >= LIMIAR_GATILHO,
            rt: estado.rt >= LIMIAR_GATILHO,
            analogico_cima: estado.ly >= LIMIAR_ANALOGICO,
            analogico_baixo: estado.ly <= -LIMIAR_ANALOGICO,
            analogico_esquerda: estado.lx <= -LIMIAR_ANALOGICO,
            analogico_direita: estado.lx >= LIMIAR_ANALOGICO,
        }
    }

    /// Cada entrada: nome e se está apertada.
    fn lista(&self) -> Vec<(&'static str, bool)> {
        let mut lista: Vec<(&'static str, bool)> = NOMES.iter().map(|(mascara, nome)| (*nome, self.botoes & mascara != 0)).collect();
        lista.push(("LT", self.lt));
        lista.push(("RT", self.rt));
        lista.push(("Analógico ↑", self.analogico_cima));
        lista.push(("Analógico ↓", self.analogico_baixo));
        lista.push(("Analógico ←", self.analogico_esquerda));
        lista.push(("Analógico →", self.analogico_direita));
        lista
    }
}

/// O que mudou entre dois instantes: `+Nome` (apertou) e `-Nome` (soltou).
fn diferencas(antes: &Apertos, depois: &Apertos) -> Vec<String> {
    antes
        .lista()
        .into_iter()
        .zip(depois.lista())
        .filter(|((_, a), (_, d))| a != d)
        .map(|((nome, _), (_, d))| format!("{}{nome}", if d { '+' } else { '-' }))
        .collect()
}

pub struct Gravador {
    arquivo: PathBuf,
    proxima_checagem: Instant,
    ligado_desde: Option<Instant>,
    anterior: Apertos,
    ultimo_foco: Option<u32>,
}

impl Gravador {
    pub fn new() -> Self {
        Gravador {
            arquivo: std::env::temp_dir().join("fifa_gravar_controle.pedido"),
            proxima_checagem: Instant::now(),
            ligado_desde: None,
            anterior: Apertos::default(),
            ultimo_foco: None,
        }
    }

    /// Chamado a cada frame com o controle de agora e o jogador em foco no
    /// jogo (`(id, nome)`), se a Central o leu.
    pub fn registrar(&mut self, controle: Option<EstadoControle>, foco: Option<(u32, &str)>) {
        let agora = Instant::now();
        if agora >= self.proxima_checagem {
            self.proxima_checagem = agora + INTERVALO_CHECAGEM;
            let pedido = self.arquivo.exists();
            match (pedido, self.ligado_desde) {
                (true, None) => {
                    self.ligado_desde = Some(agora);
                    self.anterior = Apertos::default();
                    self.ultimo_foco = None;
                    tracing::info!("[gravador] ligado: aperte os botões como de costume; apague {} para parar.", self.arquivo.display());
                }
                (false, Some(_)) => {
                    self.ligado_desde = None;
                    tracing::info!("[gravador] desligado.");
                }
                _ => {}
            }
        }
        let Some(inicio) = self.ligado_desde else { return };
        let ms = agora.duration_since(inicio).as_millis();
        if let Some(estado) = controle {
            let apertos = Apertos::de(&estado);
            let mudancas = diferencas(&self.anterior, &apertos);
            if !mudancas.is_empty() {
                tracing::info!("[gravador] t={ms}ms {}", mudancas.join(" "));
            }
            self.anterior = apertos;
        }
        let foco_id = foco.map(|(id, _)| id);
        if foco_id != self.ultimo_foco {
            self.ultimo_foco = foco_id;
            if let Some((id, nome)) = foco {
                tracing::info!("[gravador] t={ms}ms foco no jogo: {id} \"{nome}\"");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn estado(botoes: u16) -> EstadoControle {
        EstadoControle { botoes, ..Default::default() }
    }

    #[test]
    fn presses_and_releases_are_named_in_the_order_of_the_table() {
        let nada = Apertos::de(&estado(0));
        let a_e_baixo = Apertos::de(&estado(botao::A | botao::DPAD_BAIXO));
        assert_eq!(diferencas(&nada, &a_e_baixo), ["+D-pad ↓", "+A"]);
        assert_eq!(diferencas(&a_e_baixo, &nada), ["-D-pad ↓", "-A"]);
        assert!(diferencas(&a_e_baixo, &a_e_baixo).is_empty(), "nada mudou");
    }

    #[test]
    fn triggers_and_the_left_stick_count_only_past_their_thresholds() {
        let leve = EstadoControle { lt: LIMIAR_GATILHO - 1, ly: LIMIAR_ANALOGICO - 1, ..Default::default() };
        assert!(diferencas(&Apertos::default(), &Apertos::de(&leve)).is_empty());
        let firme = EstadoControle { lt: 255, rt: LIMIAR_GATILHO, ly: -LIMIAR_ANALOGICO, lx: LIMIAR_ANALOGICO, ..Default::default() };
        assert_eq!(diferencas(&Apertos::default(), &Apertos::de(&firme)), ["+LT", "+RT", "+Analógico ↓", "+Analógico →"]);
    }
}
