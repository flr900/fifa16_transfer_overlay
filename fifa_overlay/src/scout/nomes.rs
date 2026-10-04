//! `scout::nomes` — nome de cada Olheiro gerado para o mercado (Épico 5,
//! item 1: "nome personalizado por Olheiro"). O nome sai da nação dele
//! (código ISO do banco do FIFA) e da semente da oferta: o mesmo Olheiro
//! tem sempre o mesmo nome. O jogador pode trocar o nome na confirmação
//! da contratação.

/// Grupos de nomes por idioma/região.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Grupo {
    Lusofono,
    Hispanico,
    Ingles,
    Frances,
    Italiano,
    Alemao,
    Holandes,
    Nordico,
    Eslavo,
    Arabe,
    Turco,
    Japones,
    Coreano,
    Chines,
    AfricaOcidental,
}

fn grupo(iso: &str) -> Grupo {
    match iso.to_ascii_uppercase().as_str() {
        "BR" | "PT" | "AO" | "MZ" | "CV" | "GW" => Grupo::Lusofono,
        "ES" | "AR" | "MX" | "CO" | "UY" | "CL" | "PE" | "PY" | "VE" | "EC" | "BO" | "CR" | "HN" | "SV" | "GT" | "PA" | "CU" | "DO" | "NI" => {
            Grupo::Hispanico
        }
        "FR" | "BE" | "CI" | "SN" | "CM" | "ML" | "GN" | "BF" | "CD" | "CG" | "GA" | "LU" | "HT" => Grupo::Frances,
        "IT" | "SM" | "MT" => Grupo::Italiano,
        "DE" | "AT" | "CH" | "LI" => Grupo::Alemao,
        "NL" => Grupo::Holandes,
        "SE" | "NO" | "DK" | "FI" | "IS" => Grupo::Nordico,
        "RU" | "PL" | "CZ" | "SK" | "HR" | "RS" | "UA" | "SI" | "BA" | "BG" | "ME" | "MK" | "BY" => Grupo::Eslavo,
        "SA" | "AE" | "EG" | "MA" | "TN" | "DZ" | "QA" | "JO" | "IQ" | "KW" | "OM" | "BH" | "LY" | "SY" | "LB" => Grupo::Arabe,
        "TR" => Grupo::Turco,
        "JP" => Grupo::Japones,
        "KR" | "KP" => Grupo::Coreano,
        "CN" | "TW" | "HK" => Grupo::Chines,
        "NG" | "GH" | "SL" | "LR" | "GM" | "KE" | "UG" | "ZM" | "ZW" | "TZ" => Grupo::AfricaOcidental,
        _ => Grupo::Ingles,
    }
}

fn listas(grupo: Grupo) -> (&'static [&'static str], &'static [&'static str]) {
    match grupo {
        Grupo::Lusofono => (
            &["João", "Carlos", "Paulo", "Ricardo", "Marcelo", "Fernando", "Luís", "André", "Rafael", "Tiago", "Sérgio", "Eduardo", "Rodrigo", "Gustavo", "Márcio", "Hélio", "Vítor", "Renato"],
            &["Silva", "Santos", "Oliveira", "Souza", "Pereira", "Costa", "Ferreira", "Almeida", "Carvalho", "Ribeiro", "Medeiros", "Teixeira", "Barros", "Moura", "Cardoso", "Nogueira", "Rocha", "Prado"],
        ),
        Grupo::Hispanico => (
            &["Javier", "Diego", "Alejandro", "Martín", "Pablo", "Sergio", "Andrés", "Fernando", "Miguel", "Raúl", "Gonzalo", "Matías", "Nicolás", "Esteban", "Hernán", "Ignacio"],
            &["García", "Fernández", "López", "Martínez", "González", "Rodríguez", "Sánchez", "Pérez", "Romero", "Díaz", "Álvarez", "Torres", "Ruiz", "Herrera", "Castillo", "Morales"],
        ),
        Grupo::Ingles => (
            &["James", "David", "Mark", "Paul", "Steven", "Michael", "Andrew", "Richard", "Gary", "Neil", "Simon", "Peter", "Thomas", "Daniel", "Chris", "Graham"],
            &["Smith", "Johnson", "Taylor", "Brown", "Wilson", "Evans", "Walker", "Wright", "Roberts", "Hughes", "Clarke", "Turner", "Parker", "Morgan", "Cooper", "Bennett"],
        ),
        Grupo::Frances => (
            &["Jean", "Pierre", "Michel", "Laurent", "Olivier", "Nicolas", "Julien", "Thierry", "Didier", "Patrick", "Sébastien", "Frédéric", "Yannick", "Mamadou", "Moussa", "Ibrahim"],
            &["Martin", "Bernard", "Dubois", "Moreau", "Laurent", "Lefebvre", "Girard", "Rousseau", "Fontaine", "Mercier", "Diallo", "Traoré", "Koné", "Camara", "Mbaye", "Ndiaye"],
        ),
        Grupo::Italiano => (
            &["Marco", "Luca", "Giuseppe", "Andrea", "Francesco", "Alessandro", "Stefano", "Roberto", "Paolo", "Massimo", "Fabio", "Claudio", "Davide", "Simone"],
            &["Rossi", "Russo", "Ferrari", "Esposito", "Bianchi", "Romano", "Colombo", "Ricci", "Marino", "Greco", "Bruno", "Gallo", "Conti", "De Luca", "Costa", "Fontana"],
        ),
        Grupo::Alemao => (
            &["Thomas", "Michael", "Stefan", "Andreas", "Markus", "Jürgen", "Uwe", "Matthias", "Jan", "Tobias", "Florian", "Ralf", "Sven", "Dieter"],
            &["Müller", "Schmidt", "Schneider", "Fischer", "Weber", "Meyer", "Wagner", "Becker", "Hoffmann", "Schulz", "Koch", "Richter", "Klein", "Wolf", "Neumann", "Krause"],
        ),
        Grupo::Holandes => (
            &["Jan", "Pieter", "Dirk", "Ruud", "Frank", "Johan", "Marco", "Dennis", "Arjen", "Wesley", "Bas", "Erik"],
            &["de Jong", "Jansen", "de Vries", "van Dijk", "Bakker", "Visser", "Smit", "Meijer", "de Boer", "Mulder", "Bos", "van den Berg", "Kuiper", "Vos"],
        ),
        Grupo::Nordico => (
            &["Lars", "Anders", "Erik", "Johan", "Henrik", "Magnus", "Nils", "Ole", "Mikael", "Jesper", "Kristian", "Bjørn"],
            &["Johansson", "Andersson", "Nilsson", "Hansen", "Larsen", "Jensen", "Olsen", "Lindqvist", "Berg", "Nielsen", "Karlsson", "Halvorsen"],
        ),
        Grupo::Eslavo => (
            &["Ivan", "Dmitri", "Sergei", "Andrei", "Pavel", "Tomasz", "Marek", "Piotr", "Luka", "Dragan", "Milan", "Nikola", "Petr", "Jakub"],
            &["Ivanov", "Petrov", "Smirnov", "Kowalski", "Nowak", "Wisniewski", "Novák", "Horvat", "Kovacevic", "Jovanovic", "Popovic", "Dvorák", "Markovic", "Volkov"],
        ),
        Grupo::Arabe => (
            &["Mohamed", "Ahmed", "Youssef", "Omar", "Karim", "Hassan", "Khalid", "Tarek", "Samir", "Walid", "Nabil", "Rachid"],
            &["Al-Sayed", "Haddad", "Mansour", "Benali", "El-Amrani", "Saleh", "Nasser", "Hamdi", "Bouzid", "Farouk", "Khalil", "Aziz"],
        ),
        Grupo::Turco => (
            &["Mehmet", "Mustafa", "Ahmet", "Emre", "Burak", "Hakan", "Serkan", "Volkan", "Arda", "Cem"],
            &["Yilmaz", "Kaya", "Demir", "Sahin", "Çelik", "Yildiz", "Aydin", "Öztürk", "Arslan", "Dogan"],
        ),
        Grupo::Japones => (
            &["Hiroshi", "Takeshi", "Kenji", "Yuki", "Daisuke", "Shinji", "Kazuki", "Makoto", "Ryo", "Satoshi"],
            &["Sato", "Suzuki", "Takahashi", "Tanaka", "Watanabe", "Ito", "Nakamura", "Kobayashi", "Yamamoto", "Kato"],
        ),
        Grupo::Coreano => (
            &["Min-jun", "Ji-sung", "Dong-hyun", "Sung-min", "Jae-won", "Hyun-woo", "Young-ho", "Tae-hee"],
            &["Kim", "Lee", "Park", "Choi", "Jung", "Kang", "Cho", "Yoon"],
        ),
        Grupo::Chines => (&["Wei", "Jun", "Hao", "Lei", "Ming", "Tao", "Yong", "Jian"], &["Wang", "Li", "Zhang", "Liu", "Chen", "Yang", "Zhao", "Huang", "Zhou", "Wu"]),
        Grupo::AfricaOcidental => (
            &["Emmanuel", "Samuel", "Daniel", "Kwame", "Chinedu", "Michael", "Isaac", "Joseph", "Victor", "Kofi"],
            &["Okafor", "Mensah", "Adeyemi", "Boateng", "Owusu", "Eze", "Asante", "Oduya", "Nwosu", "Appiah"],
        ),
    }
}

/// Nome de um Olheiro da nação `iso` (vazio = lista genérica).
pub fn gerar(iso: &str, semente: u64) -> String {
    let (primeiros, sobrenomes) = listas(grupo(iso));
    let i = usize::try_from(semente % primeiros.len() as u64).unwrap_or(0);
    let j = usize::try_from((semente / 7919) % sobrenomes.len() as u64).unwrap_or(0);
    format!("{} {}", primeiros.get(i).copied().unwrap_or("Carlos"), sobrenomes.get(j).copied().unwrap_or("Silva"))
}

/// O nome digitado na confirmação, limpo (espaços nas pontas; no máximo 40
/// caracteres). Vazio volta ao gerado.
pub fn limpar(digitado: &str, gerado: &str) -> String {
    let limpo: String = digitado.trim().chars().take(40).collect();
    if limpo.is_empty() {
        gerado.to_string()
    } else {
        limpo
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_the_nation_and_are_stable() {
        assert_eq!(gerar("BR", 42), gerar("BR", 42));
        let brasileiro = gerar("BR", 3);
        let (primeiros, _) = listas(Grupo::Lusofono);
        assert!(primeiros.iter().any(|p| brasileiro.starts_with(p)), "{brasileiro}");
        assert_ne!(gerar("BR", 1), gerar("BR", 2));
        assert!(!gerar("", 9).is_empty(), "sem nação: lista genérica");
        for g in [Grupo::Lusofono, Grupo::Chines, Grupo::Coreano, Grupo::AfricaOcidental] {
            let (p, s) = listas(g);
            assert!(!p.is_empty() && !s.is_empty());
        }
    }

    /// A fonte do overlay só tem Latin-1 (`theme::FAIXAS_DE_GLIFOS`): letras
    /// como "ı" ou "ć" apareceriam como "?".
    #[test]
    fn every_name_fits_the_overlay_font() {
        let grupos = [
            Grupo::Lusofono,
            Grupo::Hispanico,
            Grupo::Ingles,
            Grupo::Frances,
            Grupo::Italiano,
            Grupo::Alemao,
            Grupo::Holandes,
            Grupo::Nordico,
            Grupo::Eslavo,
            Grupo::Arabe,
            Grupo::Turco,
            Grupo::Japones,
            Grupo::Coreano,
            Grupo::Chines,
            Grupo::AfricaOcidental,
        ];
        for g in grupos {
            let (p, s) = listas(g);
            for nome in p.iter().chain(s) {
                assert!(nome.chars().all(|c| (c as u32) <= 0xFF), "{nome}");
            }
        }
    }

    #[test]
    fn typed_names_are_trimmed_and_empty_goes_back_to_the_generated_one() {
        assert_eq!(limpar("  Zé do Scout ", "Carlos Silva"), "Zé do Scout");
        assert_eq!(limpar("   ", "Carlos Silva"), "Carlos Silva");
        assert_eq!(limpar(&"a".repeat(80), "x").chars().count(), 40);
    }
}
