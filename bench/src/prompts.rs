use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Expect {
    Ui,
    NoUi,
    Either,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Prompt {
    pub id: String,
    pub locale: &'static str,
    pub category: &'static str,
    pub expect: Expect,
    pub text: &'static str,
}

pub const LOCALES: [&str; 4] = ["en", "uz-Latn", "uz-Cyrl", "ru"];

pub const CATEGORIES: [(&str, Expect); 8] = [
    ("bills", Expect::Ui),
    ("pay", Expect::Ui),
    ("offers", Expect::Ui),
    ("chart", Expect::Ui),
    ("phone-form", Expect::Ui),
    ("ambiguous", Expect::Either),
    ("no-ui", Expect::NoUi),
    ("bill-detail", Expect::Ui),
];

const TEXTS: [[&str; 8]; 4] = [
    [
        "What do I owe this month? Show me all my bills.",
        "Pay my gas bill.",
        "Compare the internet tariffs and let me pick one.",
        "Show a chart of my electricity usage over the last six months.",
        "I want to change the phone number for payment notifications. Ask me for the new number.",
        "the water thing",
        "What does kWh stand for?",
        "How much is my internet bill and when is it due?",
    ],
    [
        "Bu oy qancha to'lashim kerak? Hamma hisoblarimni ko'rsat.",
        "Gaz uchun to'lovni qil.",
        "Internet tariflarini solishtir, bittasini tanlamoqchiman.",
        "Oxirgi olti oydagi gaz sarfimni grafikda ko'rsat.",
        "To'lov xabarlari keladigan telefon raqamimni o'zgartirmoqchiman. Yangi raqamni so'ra.",
        "suv haqida",
        "Rahmat, hozircha boshqa savolim yo'q.",
        "Elektr hisobim qancha va qachongacha to'lashim kerak?",
    ],
    [
        "Бу ой қанча тўлашим керак? Ҳамма ҳисобларимни кўрсат.",
        "Электр учун тўловни қил.",
        "Интернет тарифларини солиштир, биттасини танламоқчиман.",
        "Охирги олти ойдаги сув сарфимни графикда кўрсат.",
        "Тўлов хабарлари келадиган телефон рақамимни ўзгартирмоқчиман. Янги рақамни сўра.",
        "интернет",
        "кВт·соат нима дегани?",
        "Газ ҳисобим қанча ва қачонгача тўлашим керак?",
    ],
    [
        "Сколько я должен заплатить в этом месяце? Покажи все счета.",
        "Оплати счёт за воду.",
        "Сравни тарифы на интернет, хочу выбрать один.",
        "Покажи график моих платежей за газ за последние полгода.",
        "Хочу поменять номер телефона для уведомлений об оплате. Спроси у меня новый номер.",
        "а что с этим счётом?",
        "Спасибо, на сегодня всё.",
        "Сколько нужно заплатить за воду и до какого числа?",
    ],
];

pub fn prompts() -> Vec<Prompt> {
    let mut out = Vec::with_capacity(LOCALES.len() * CATEGORIES.len());
    for round in 0..CATEGORIES.len() {
        for (index, locale) in LOCALES.iter().enumerate() {
            let category = (round + index) % CATEGORIES.len();
            let (name, expect) = CATEGORIES[category];
            out.push(Prompt {
                id: format!("{}-{name}", locale.to_ascii_lowercase()),
                locale,
                category: name,
                expect,
                text: TEXTS[index][category],
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn every_locale_gets_every_category_once() {
        let all = prompts();
        assert!(all.len() >= 30);
        let pairs: BTreeSet<(&str, &str)> = all
            .iter()
            .map(|prompt| (prompt.locale, prompt.category))
            .collect();
        assert_eq!(pairs.len(), all.len());
        let ids: BTreeSet<&str> = all.iter().map(|prompt| prompt.id.as_str()).collect();
        assert_eq!(ids.len(), all.len());
    }

    #[test]
    fn the_first_four_cover_four_locales_and_four_categories() {
        let first: Vec<Prompt> = prompts().into_iter().take(4).collect();
        let locales: BTreeSet<&str> = first.iter().map(|prompt| prompt.locale).collect();
        let categories: BTreeSet<&str> = first.iter().map(|prompt| prompt.category).collect();
        assert_eq!(locales.len(), 4);
        assert_eq!(categories.len(), 4);
    }

    #[test]
    fn some_prompts_should_not_produce_ui() {
        assert!(
            prompts()
                .iter()
                .filter(|prompt| prompt.expect == Expect::NoUi)
                .count()
                >= 4
        );
    }
}
