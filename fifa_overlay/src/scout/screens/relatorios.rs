//! Aba Relatórios. Por enquanto só o placeholder neutro; os Relatórios
//! (Tabular/Cards) chegam no Épico 2.

use imgui::Ui;

use super::theme::Fonts;

pub fn render(ui: &Ui, fonts: Option<&Fonts>) {
    super::aba_vazia(ui, fonts);
}
