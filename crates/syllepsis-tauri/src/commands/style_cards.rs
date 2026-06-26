//! Commands for style card CRUD (stored as JSON files in `_style_cards/` inside the book).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::State;

use crate::state::AppState;

/// A style card stored on disk: the card data plus a unique id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyleCardEntry {
    pub id: String,
    #[serde(default = "default_version")]
    pub version: u32,
    pub name: String,
    pub short_description: String,
    pub verbosity: String,
    pub perspective: String,
    pub reading_level: String,
    pub voice: String,
    #[serde(default)]
    pub patterns: Vec<StylePattern>,
    #[serde(default)]
    pub exemplars: Vec<StyleExemplar>,
    #[serde(default)]
    pub source_urls: Vec<String>,
}

fn default_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StylePattern {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyleExemplar {
    pub text: String,
}

pub(crate) fn builtin_style_cards() -> Vec<StyleCardEntry> {
    vec![
        StyleCardEntry {
            id: "builtin:administrative-email".into(),
            version: 1,
            name: "Administrative Email".into(),
            short_description: "A formal, objective, and highly standardized communication used to convey policies or mandatory actions. Tone is polite, neutral, slightly bureaucratic, and completely devoid of personal emotion.".into(),
            verbosity: "succinct".into(),
            perspective: "first_person_plural".into(),
            reading_level: "accessible".into(),
            voice: "passive".into(),
            patterns: vec![
                StylePattern { text: "Rely on corporate buzzwords, softened directives, and standardized greetings/sign-offs (e.g., \"Please be advised\", \"Going forward\").".into() },
                StylePattern { text: "Favour the passive voice to distance the sender from the policy or mandate.".into() },
                StylePattern { text: "Use bulleted lists for clarity and to outline specific steps or changes.".into() },
                StylePattern { text: "Avoid exclamation marks, slang, personal anecdotes, or any tone that could be construed as confrontational or overly enthusiastic.".into() },
            ],
            exemplars: vec![
                StyleExemplar { text: "Please be advised that the new expense reporting guidelines will take effect in Q3. All employees are required to submit outstanding receipts by EOD Friday.".into() },
                StyleExemplar { text: "It has come to our attention that security badges are not being worn visibly. Going forward, compliance will be strictly monitored.".into() },
            ],
            source_urls: vec![],
        },
        StyleCardEntry {
            id: "builtin:ted-talk".into(),
            version: 1,
            name: "TED Talk".into(),
            short_description: "An accessible, highly engaging, and intellectually stimulating presentation designed to share a \"big idea.\" Tone is optimistic, deeply empathetic, narrative-driven, and conversational yet rehearsed.".into(),
            verbosity: "standard".into(),
            perspective: "first_person_singular".into(),
            reading_level: "accessible".into(),
            voice: "active".into(),
            patterns: vec![
                StylePattern { text: "Begin with a relatable, vulnerable personal anecdote or a surprising, counter-intuitive question to hook the audience.".into() },
                StylePattern { text: "Transition frequently from \"I\" (personal experience) to \"we\" (shared human experience) to build a bridge of collective potential.".into() },
                StylePattern { text: "Translate complex data, academic research, or scientific concepts into simple, striking metaphors.".into() },
                StylePattern { text: "Avoid heavy academic jargon, monotone data dumping, or aggressive sales pitches.".into() },
            ],
            exemplars: vec![
                StyleExemplar { text: "A few years ago, I found myself sitting in my car, crying over a spreadsheet. And that's when I realized: everything I thought I knew about vulnerability was completely wrong.".into() },
                StyleExemplar { text: "So, what does this mean for us? It means we have the power to rewrite our cognitive scripts. Imagine a world where our failures are just data.".into() },
            ],
            source_urls: vec![],
        },
        StyleCardEntry {
            id: "builtin:shakespearean-narrator".into(),
            version: 1,
            name: "Shakespearean Narrator".into(),
            short_description: "A formal, dramatic, and authoritative guide who sets the scene, bridges gaps in time, and appeals directly to the audience. The tone is grand, inviting, and slightly apologetic about the limitations of the medium.".into(),
            verbosity: "expansive".into(),
            perspective: "first_person_plural".into(),
            reading_level: "advanced".into(),
            voice: "active".into(),
            patterns: vec![
                StylePattern { text: "Employ iambic pentameter and end-capped rhyming couplets to elevate the prologue or epilogue.".into() },
                StylePattern { text: "Use grand imagery and classical allusions to establish the setting, scale, and stakes of the narrative.".into() },
                StylePattern { text: "Directly address the audience, frequently commanding them to use their imagination to fill in the visual gaps.".into() },
                StylePattern { text: "Avoid modern slang, contractions, and internal emotional disclosures.".into() },
            ],
            exemplars: vec![
                StyleExemplar { text: "Two households, both alike in dignity, In fair Verona, where we lay our scene, From ancient grudge break to new mutiny\u{2026}".into() },
                StyleExemplar { text: "Piece out our imperfections with your thoughts; Into a thousand parts divide one man, And make imaginary puissance.".into() },
            ],
            source_urls: vec![],
        },
        StyleCardEntry {
            id: "builtin:shakespearean-comic-sidekick".into(),
            version: 1,
            name: "Shakespearean Comic Sidekick".into(),
            short_description: "A lively, irreverent, and quick-witted trickster or cynic who disrupts serious moments with wordplay. The tone is mocking, bawdy, playful, and highly conversational.".into(),
            verbosity: "expansive".into(),
            perspective: "first_person_singular".into(),
            reading_level: "advanced".into(),
            voice: "active".into(),
            patterns: vec![
                StylePattern { text: "Rely heavily on puns, double entendres, and bawdy innuendo.".into() },
                StylePattern { text: "Switch fluidly between rapid-fire prose for banter and rhyming couplets for magical or mischievous incantations.".into() },
                StylePattern { text: "Mock the earnestness or romantic idealism of other characters using vivid, earthy metaphors.".into() },
                StylePattern { text: "Avoid solemnity, straightforward declarations, and passive observation.".into() },
            ],
            exemplars: vec![
                StyleExemplar { text: "O, then, I see Queen Mab hath been with you. She is the fairies' midwife, and she comes in shape no bigger than an agate-stone.".into() },
                StyleExemplar { text: "Lord, what fools these mortals be!".into() },
            ],
            source_urls: vec![],
        },
        StyleCardEntry {
            id: "builtin:shakespearean-hero".into(),
            version: 1,
            name: "Shakespearean Hero".into(),
            short_description: "Passionate, earnest, and deeply introspective, often wrestling with heavy burdens of duty, love, or honor. The tone ranges from desperately romantic to fiercely inspirational, usually highly formal and poetic.".into(),
            verbosity: "expansive".into(),
            perspective: "first_person_soliloquy".into(),
            reading_level: "advanced".into(),
            voice: "active".into(),
            patterns: vec![
                StylePattern { text: "Use sweeping soliloquies to explore internal conflict, moral dilemmas, and existential questions.".into() },
                StylePattern { text: "Employ extended metaphors (conceits) to describe love, war, or the human condition.".into() },
                StylePattern { text: "Use rhetorical questions and exclamations to convey intense emotional turmoil.".into() },
                StylePattern { text: "Avoid brevity, emotional detachment, and crude or lowbrow humor.".into() },
            ],
            exemplars: vec![
                StyleExemplar { text: "But, soft! what light through yonder window breaks? It is the east, and Juliet is the sun.".into() },
                StyleExemplar { text: "To be, or not to be, that is the question: Whether 'tis nobler in the mind to suffer the slings and arrows of outrageous fortune\u{2026}".into() },
            ],
            source_urls: vec![],
        },
        StyleCardEntry {
            id: "builtin:shakespearean-villain".into(),
            version: 1,
            name: "Shakespearean Villain".into(),
            short_description: "Manipulative, deeply cynical, and overtly ambitious, revealing their true malicious nature only to the audience. Tone is chillingly pragmatic, deceitful, and mockingly polite to their victims.".into(),
            verbosity: "expansive".into(),
            perspective: "first_person_soliloquy".into(),
            reading_level: "advanced".into(),
            voice: "active".into(),
            patterns: vec![
                StylePattern { text: "Use stark, predatory, or disease-related imagery (e.g., snakes, spiders, poison, infection).".into() },
                StylePattern { text: "Employ dramatic irony by outlining evil plots to the audience while feigning extreme loyalty and honesty to other characters.".into() },
                StylePattern { text: "Frame heinous acts as logical necessities or natural rights, justifying them with twisted logic.".into() },
                StylePattern { text: "Avoid genuine expressions of remorse, empathy, or hesitation.".into() },
            ],
            exemplars: vec![
                StyleExemplar { text: "I am not what I am.".into() },
                StyleExemplar { text: "And therefore, since I cannot prove a lover, to entertain these fair well-spoken days, I am determined to prove a villain.".into() },
            ],
            source_urls: vec![],
        },
        StyleCardEntry {
            id: "builtin:natural-history-documentary".into(),
            version: 1,
            name: "Natural History Documentary Narrator".into(),
            short_description: "An observant, reverent, accessible third-person voice for natural processes, animal behavior, and environmental drama. Tone is calm, curious, precise, and gently suspenseful; formality is polished but conversational.".into(),
            verbosity: "standard".into(),
            perspective: "third_person_objective".into(),
            reading_level: "accessible".into(),
            voice: "active".into(),
            patterns: vec![
                StylePattern { text: "Use present tense to create immediacy: the subject is not merely described, but encountered in the act of surviving, searching, waiting, or adapting.".into() },
                StylePattern { text: "Frame ordinary behavior as consequential drama, but avoid melodrama; tension should arise from ecological stakes, scarcity, timing, or vulnerability.".into() },
                StylePattern { text: "Move from wide context to close detail: habitat, season, constraint, then the individual animal or organism.".into() },
                StylePattern { text: "Prefer precise, concrete verbs and natural-cause explanations; avoid slang, irony, and ornate metaphor.".into() },
            ],
            exemplars: vec![
                StyleExemplar { text: "At the edge of the reedbed, the heron waits. Every movement must justify its cost, for in this cold light even patience consumes energy.".into() },
                StyleExemplar { text: "Beneath the fallen leaves, a small world is already awake. Fungi thread through the soil, turning last year's growth into the beginning of the next.".into() },
            ],
            source_urls: vec![],
        },
        StyleCardEntry {
            id: "builtin:jane-austen".into(),
            version: 1,
            name: "Jane Austen".into(),
            short_description: "A witty, socially observant third-person omniscient voice centered on manners, judgment, self-deception, and reputation. Tone is ironic, elegant, psychologically exact, and formally restrained.".into(),
            verbosity: "standard".into(),
            perspective: "third_person_omniscient".into(),
            reading_level: "advanced".into(),
            voice: "active".into(),
            patterns: vec![
                StylePattern { text: "Use free indirect discourse: let narration slide into a character's assumptions, vanity, or rationalizations without explicit quotation.".into() },
                StylePattern { text: "Build irony through polite understatement; the sentence should often appear decorous while quietly exposing folly.".into() },
                StylePattern { text: "Anchor conflict in social interpretation: visits, letters, introductions, income, family expectation, rank, propriety, and marriageability.".into() },
                StylePattern { text: "Favor balanced, periodic sentences with qualifications and reversals; avoid modern slang, abrupt minimalism, and overt moralizing.".into() },
            ],
            exemplars: vec![
                StyleExemplar { text: "Mrs. Harcourt had long considered herself immune to flattery, by which she meant only that she preferred it carefully disguised.".into() },
                StyleExemplar { text: "Edward's silence was judged by his aunt to be prudence, by his sister to be indifference, and by himself, when he could bear the reflection, to be cowardice.".into() },
            ],
            source_urls: vec![],
        },
        StyleCardEntry {
            id: "builtin:tolkien".into(),
            version: 1,
            name: "J. R. R. Tolkien".into(),
            short_description: "A grave, mythic, third-person omniscient voice suited to journeys, ancient places, moral testing, and the long memory of peoples and lands. Tone is elevated, elegiac, earnest, and expansive.".into(),
            verbosity: "expansive".into(),
            perspective: "third_person_omniscient".into(),
            reading_level: "advanced".into(),
            voice: "active".into(),
            patterns: vec![
                StylePattern { text: "Give places historical depth: landscapes should seem inhabited by memory, old names, lost kingdoms, forgotten craft, or songs half-remembered.".into() },
                StylePattern { text: "Use elevated diction and rhythmic clauses, especially for solemn moments; avoid cynicism, contemporary idiom, and clipped modern banter.".into() },
                StylePattern { text: "Contrast humble agents with vast stakes; courage often appears as endurance, loyalty, mercy, or refusal to abandon a duty.".into() },
                StylePattern { text: "Let description carry moral atmosphere: light, shadow, wind, stone, trees, roads, stars, and thresholds should imply danger or hope.".into() },
            ],
            exemplars: vec![
                StyleExemplar { text: "Beyond the last tilled field the road bent northward, and there the wind came down cold from the hills, bearing the smell of rain and stone.".into() },
                StyleExemplar { text: "Few now remembered the name of that tower, though shepherds still avoided its shadow when evening gathered in the valley.".into() },
            ],
            source_urls: vec![],
        },
    ]
}

fn cards_dir(book_root: &std::path::Path) -> PathBuf {
    book_root.join("_style_cards")
}

fn card_path(book_root: &std::path::Path, id: &str) -> PathBuf {
    cards_dir(book_root).join(format!("{id}.json"))
}

fn ensure_dir(path: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|e| format!("create _style_cards dir: {e}"))
}

pub(crate) fn style_card_for_book(
    book_root: &std::path::Path,
    id: &str,
) -> Result<Option<StyleCardEntry>, String> {
    let path = card_path(book_root, id);
    if path.exists() {
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        return serde_json::from_str::<StyleCardEntry>(&text)
            .map(Some)
            .map_err(|e| format!("parse style card {id}: {e}"));
    }
    Ok(builtin_style_cards().into_iter().find(|c| c.id == id))
}

/// List all style cards: built-ins first, then user cards from the open book.
#[tauri::command]
pub fn list_style_cards(state: State<AppState>) -> Result<Vec<StyleCardEntry>, String> {
    let mut cards: Vec<StyleCardEntry> = builtin_style_cards();

    let guard = state.book.lock().unwrap();
    let book = guard.as_ref().ok_or("no book is open")?;
    let dir = cards_dir(&book.root);
    if dir.exists() {
        for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
            match serde_json::from_str::<StyleCardEntry>(&text) {
                Ok(card) => cards.push(card),
                Err(e) => tracing::warn!("skipping malformed style card {:?}: {e}", path),
            }
        }
    }
    Ok(cards)
}

/// Save (create or update) a style card.
#[tauri::command]
pub fn save_style_card(
    state: State<AppState>,
    card: StyleCardEntry,
) -> Result<StyleCardEntry, String> {
    let guard = state.book.lock().unwrap();
    let book = guard.as_ref().ok_or("no book is open")?;
    let dir = cards_dir(&book.root);
    ensure_dir(&dir)?;
    let id = if card.id.is_empty() {
        format!("sc-{}", ulid::Ulid::new().to_string().to_lowercase())
    } else {
        card.id.clone()
    };
    let saved = StyleCardEntry {
        id: id.clone(),
        ..card
    };
    let text = serde_json::to_string_pretty(&saved).map_err(|e| e.to_string())?;
    std::fs::write(card_path(&book.root, &id), text).map_err(|e| e.to_string())?;
    Ok(saved)
}

/// Delete a style card by id.
#[tauri::command]
pub fn delete_style_card(state: State<AppState>, id: String) -> Result<(), String> {
    let guard = state.book.lock().unwrap();
    let book = guard.as_ref().ok_or("no book is open")?;
    let path = card_path(&book.root, &id);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}
