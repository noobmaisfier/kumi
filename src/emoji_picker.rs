//! Emoji picker (`kumizo toggle kumi-emoji`): search, category filter, grid navigation,
//! and clipboard copy via the `Clipboard` service.

use gpui::prelude::*;
use gpui::{
    div, px, AnimationExt, ClickEvent, Context, Entity, FocusHandle, FontWeight, KeyDownEvent,
    ScrollHandle, Window,
};
use kumizo_kit::prelude::*;

use crate::{
    icons,
    theme::pal,
    ui::{self, ids},
};

const COLS: usize = 10;
const EMOJI_FONT: &str = "Noto Color Emoji";

/// Strip variation selector U+FE0F for display so `cosmic-text` does not fall
/// back from `NotoColorEmoji` (whose cmap omits U+FE0F) to monochrome symbol fonts.
fn display_glyph(glyph: &'static str) -> String {
    glyph.replace('\u{FE0F}', "")
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Category {
    All,
    Smileys,
    People,
    Nature,
    Food,
    Activities,
    Travel,
    Objects,
    Symbols,
    Flags,
}

impl Category {
    const ALL_TABS: [Category; 10] = [
        Category::All,
        Category::Smileys,
        Category::People,
        Category::Nature,
        Category::Food,
        Category::Activities,
        Category::Travel,
        Category::Objects,
        Category::Symbols,
        Category::Flags,
    ];

    fn label(self) -> &'static str {
        match self {
            Category::All => "All",
            Category::Smileys => "Smileys",
            Category::People => "People",
            Category::Nature => "Nature",
            Category::Food => "Food",
            Category::Activities => "Activities",
            Category::Travel => "Travel",
            Category::Objects => "Objects",
            Category::Symbols => "Symbols",
            Category::Flags => "Flags",
        }
    }

    fn next(self) -> Self {
        let idx = Self::ALL_TABS.iter().position(|&c| c == self).unwrap_or(0);
        Self::ALL_TABS[(idx + 1) % Self::ALL_TABS.len()]
    }

    fn prev(self) -> Self {
        let idx = Self::ALL_TABS.iter().position(|&c| c == self).unwrap_or(0);
        Self::ALL_TABS[(idx + Self::ALL_TABS.len() - 1) % Self::ALL_TABS.len()]
    }
}

#[derive(Clone, Copy)]
struct EmojiEntry {
    glyph: &'static str,
    name: &'static str,
    keywords: &'static str,
    category: Category,
}

const fn e(
    glyph: &'static str,
    name: &'static str,
    keywords: &'static str,
    category: Category,
) -> EmojiEntry {
    EmojiEntry {
        glyph,
        name,
        keywords,
        category,
    }
}

static EMOJIS: &[EmojiEntry] = &[
    // ── Smileys ──────────────────────────────────────────────────────────────
    e("😀", "grinning face", "smile happy joy", Category::Smileys),
    e("😃", "grinning face with big eyes", "smile happy", Category::Smileys),
    e("😄", "grinning face with smiling eyes", "smile happy laugh", Category::Smileys),
    e("😁", "beaming face", "grin smile teeth", Category::Smileys),
    e("😆", "grinning squinting face", "laugh haha xd", Category::Smileys),
    e("😅", "grinning face with sweat", "nervous relief pheww", Category::Smileys),
    e("🤣", "rolling on the floor laughing", "rofl lol lmao", Category::Smileys),
    e("😂", "face with tears of joy", "lol laugh cry happy", Category::Smileys),
    e("🙂", "slightly smiling face", "smile ok", Category::Smileys),
    e("🙃", "upside-down face", "silly sarcasm", Category::Smileys),
    e("😉", "winking face", "wink flirt", Category::Smileys),
    e("😊", "smiling face with smiling eyes", "blush happy warm", Category::Smileys),
    e("😇", "smiling face with halo", "angel innocent", Category::Smileys),
    e("🥰", "smiling face with hearts", "love adore crush", Category::Smileys),
    e("😍", "smiling face with heart-eyes", "love heart crush", Category::Smileys),
    e("🤩", "star-struck", "wow amazed excited", Category::Smileys),
    e("😘", "face blowing a kiss", "kiss love", Category::Smileys),
    e("😋", "face savoring food", "yum delicious tasty", Category::Smileys),
    e("😛", "face with tongue", "playful silly", Category::Smileys),
    e("😜", "winking face with tongue", "crazy playful", Category::Smileys),
    e("🤪", "zany face", "crazy goofy wild", Category::Smileys),
    e("🤨", "face with raised eyebrow", "skeptical sus hmm", Category::Smileys),
    e("🧐", "face with monocle", "inspect fancy class", Category::Smileys),
    e("🤓", "nerd face", "geek smart glasses", Category::Smileys),
    e("😎", "smiling face with sunglasses", "cool chill shades", Category::Smileys),
    e("🥳", "partying face", "party celebrate birthday", Category::Smileys),
    e("😏", "smirking face", "smug sly", Category::Smileys),
    e("😒", "unamused face", "meh annoyed bored", Category::Smileys),
    e("😞", "disappointed face", "sad down", Category::Smileys),
    e("😔", "pensive face", "sad thoughtful", Category::Smileys),
    e("😟", "worried face", "anxious concern", Category::Smileys),
    e("😕", "confused face", "unsure", Category::Smileys),
    e("🙁", "slightly frowning face", "sad frown", Category::Smileys),
    e("😣", "persevering face", "struggle", Category::Smileys),
    e("😖", "confounded face", "frustrated", Category::Smileys),
    e("😫", "tired face", "exhausted", Category::Smileys),
    e("😩", "weary face", "tired groan", Category::Smileys),
    e("🥺", "pleading face", "puppy eyes please", Category::Smileys),
    e("😢", "crying face", "sad tear", Category::Smileys),
    e("😭", "loudly crying face", "sob sad tears", Category::Smileys),
    e("😤", "face with steam from nose", "proud angry hmph", Category::Smileys),
    e("😠", "angry face", "mad upset", Category::Smileys),
    e("😡", "pouting face", "rage furious red", Category::Smileys),
    e("🤬", "face with symbols on mouth", "cursing swear", Category::Smileys),
    e("🤯", "exploding head", "mind blown shocked", Category::Smileys),
    e("😳", "flushed face", "embarrassed shy", Category::Smileys),
    e("🥵", "hot face", "heat sweat", Category::Smileys),
    e("🥶", "cold face", "freezing ice", Category::Smileys),
    e("😱", "face screaming in fear", "omg shock scared", Category::Smileys),
    e("😨", "fearful face", "scared afraid", Category::Smileys),
    e("😰", "anxious face with sweat", "nervous", Category::Smileys),
    e("🤔", "thinking face", "hmm think wonder", Category::Smileys),
    e("🤫", "shushing face", "quiet secret shh", Category::Smileys),
    e("🤭", "face with hand over mouth", "oops giggle", Category::Smileys),
    e("🫡", "saluting face", "salute respect", Category::Smileys),
    e("🫠", "melting face", "melt awkward", Category::Smileys),
    e("😴", "sleeping face", "sleep zzz tired", Category::Smileys),
    e("🤤", "drooling face", "yum hungry", Category::Smileys),
    e("🤢", "nauseated face", "sick gross", Category::Smileys),
    e("🤮", "face vomiting", "sick puke", Category::Smileys),
    e("🤧", "sneezing face", "sick achoo", Category::Smileys),
    e("🤒", "face with thermometer", "sick fever", Category::Smileys),
    e("🤕", "face with head-bandage", "hurt injured", Category::Smileys),
    e("🤠", "cowboy hat face", "yeehaw western", Category::Smileys),
    e("🤡", "clown face", "funny silly", Category::Smileys),
    e("👻", "ghost", "spooky halloween boo", Category::Smileys),
    e("💀", "skull", "dead skeleton rip", Category::Smileys),
    e("👽", "alien", "ufo space", Category::Smileys),
    e("🤖", "robot", "bot ai tech", Category::Smileys),
    e("💩", "pile of poo", "poop", Category::Smileys),

    // ── People & Gestures ────────────────────────────────────────────────────
    e("👋", "waving hand", "hello hi bye wave", Category::People),
    e("🤚", "raised back of hand", "hand stop", Category::People),
    e("✋", "raised hand", "stop high five", Category::People),
    e("👌", "OK hand", "ok nice perfect", Category::People),
    e("🤌", "pinched fingers", "italian chef kiss", Category::People),
    e("✌️", "victory hand", "peace two", Category::People),
    e("🤞", "crossed fingers", "luck hope", Category::People),
    e("🤟", "love-you gesture", "rock love", Category::People),
    e("🤘", "sign of the horns", "rock metal", Category::People),
    e("🤙", "call me hand", "shaka phone", Category::People),
    e("👈", "backhand index pointing left", "left point", Category::People),
    e("👉", "backhand index pointing right", "right point", Category::People),
    e("👆", "backhand index pointing up", "up point", Category::People),
    e("👇", "backhand index pointing down", "down point", Category::People),
    e("☝️", "index pointing up", "one idea", Category::People),
    e("👍", "thumbs up", "like yes good approve", Category::People),
    e("👎", "thumbs down", "dislike no bad", Category::People),
    e("✊", "raised fist", "power solidarity", Category::People),
    e("👊", "oncoming fist", "punch bro", Category::People),
    e("👏", "clapping hands", "clap bravo applause", Category::People),
    e("🙌", "raising hands", "yay celebrate", Category::People),
    e("🫶", "heart hands", "love care", Category::People),
    e("🤝", "handshake", "deal agree", Category::People),
    e("🙏", "folded hands", "please thanks pray namaste", Category::People),
    e("💪", "flexed biceps", "strong muscle gym", Category::People),
    e("🧠", "brain", "smart think mind", Category::People),
    e("👀", "eyes", "look see watch", Category::People),
    e("👁️", "eye", "look watch", Category::People),
    e("🫂", "people hugging", "hug comfort", Category::People),
    e("🙋", "person raising hand", "question me", Category::People),
    e("🤷", "person shrugging", "shrug idk dunno", Category::People),
    e("🤦", "person facepalming", "facepalm doh", Category::People),
    e("🧑‍💻", "technologist", "developer code programmer", Category::People),

    // ── Animals & Nature ─────────────────────────────────────────────────────
    e("🐶", "dog face", "puppy pet woof", Category::Nature),
    e("🐱", "cat face", "kitty pet meow", Category::Nature),
    e("🐭", "mouse face", "rat", Category::Nature),
    e("🐹", "hamster", "pet cute", Category::Nature),
    e("🐰", "rabbit face", "bunny", Category::Nature),
    e("🦊", "fox", "clever", Category::Nature),
    e("🐻", "bear", "animal", Category::Nature),
    e("🐼", "panda", "bamboo cute", Category::Nature),
    e("🐨", "koala", "australia", Category::Nature),
    e("🐯", "tiger face", "cat wild", Category::Nature),
    e("🦁", "lion", "king wild", Category::Nature),
    e("🐮", "cow face", "moo", Category::Nature),
    e("🐷", "pig face", "oink", Category::Nature),
    e("🐸", "frog", "ribbit pepe", Category::Nature),
    e("🐵", "monkey face", "ape", Category::Nature),
    e("🙈", "see-no-evil monkey", "shy hide", Category::Nature),
    e("🙉", "hear-no-evil monkey", "ignore", Category::Nature),
    e("🙊", "speak-no-evil monkey", "oops secret", Category::Nature),
    e("🐧", "penguin", "linux tux bird", Category::Nature),
    e("🐦", "bird", "tweet", Category::Nature),
    e("🦅", "eagle", "bird freedom", Category::Nature),
    e("🦉", "owl", "wise night", Category::Nature),
    e("🦋", "butterfly", "insect pretty", Category::Nature),
    e("🐝", "honeybee", "bee buzz", Category::Nature),
    e("🐞", "lady beetle", "bug", Category::Nature),
    e("🐢", "turtle", "slow", Category::Nature),
    e("🐍", "snake", "python", Category::Nature),
    e("🐙", "octopus", "sea ocean", Category::Nature),
    e("🐳", "spouting whale", "ocean docker", Category::Nature),
    e("🐬", "dolphin", "sea", Category::Nature),
    e("🦄", "unicorn", "magic fantasy", Category::Nature),
    e("🌸", "cherry blossom", "flower spring sakura", Category::Nature),
    e("🌹", "rose", "flower love", Category::Nature),
    e("🌻", "sunflower", "flower sun", Category::Nature),
    e("🌲", "evergreen tree", "forest pine", Category::Nature),
    e("🌴", "palm tree", "beach tropical", Category::Nature),
    e("🌵", "cactus", "desert plant", Category::Nature),
    e("🍀", "four leaf clover", "luck irish", Category::Nature),
    e("🍁", "maple leaf", "autumn fall canada", Category::Nature),
    e("🌍", "globe showing Europe-Africa", "earth world planet", Category::Nature),
    e("🌙", "crescent moon", "night sleep", Category::Nature),
    e("☀️", "sun", "bright day weather", Category::Nature),
    e("⭐", "star", "favorite rating", Category::Nature),
    e("🌟", "glowing star", "sparkle shine", Category::Nature),
    e("✨", "sparkles", "magic clean shiny new", Category::Nature),
    e("⚡", "high voltage", "lightning fast zap", Category::Nature),
    e("🔥", "fire", "hot lit flame burn", Category::Nature),
    e("💧", "droplet", "water tear", Category::Nature),
    e("❄️", "snowflake", "cold winter ice", Category::Nature),
    e("🌈", "rainbow", "pride color", Category::Nature),

    // ── Food & Drink ─────────────────────────────────────────────────────────
    e("🍎", "red apple", "fruit", Category::Food),
    e("🍊", "tangerine", "orange fruit", Category::Food),
    e("🍋", "lemon", "sour fruit", Category::Food),
    e("🍌", "banana", "fruit", Category::Food),
    e("🍉", "watermelon", "summer fruit", Category::Food),
    e("🍇", "grapes", "fruit wine", Category::Food),
    e("🍓", "strawberry", "berry fruit", Category::Food),
    e("🍒", "cherries", "fruit", Category::Food),
    e("🍑", "peach", "fruit", Category::Food),
    e("🥑", "avocado", "guacamole", Category::Food),
    e("🍅", "tomato", "vegetable pomodoro", Category::Food),
    e("🥕", "carrot", "vegetable", Category::Food),
    e("🌽", "ear of corn", "vegetable", Category::Food),
    e("🍞", "bread", "loaf toast", Category::Food),
    e("🥐", "croissant", "french pastry", Category::Food),
    e("🧀", "cheese wedge", "dairy", Category::Food),
    e("🍔", "hamburger", "burger fast food", Category::Food),
    e("🍟", "french fries", "fries snack", Category::Food),
    e("🍕", "pizza", "slice cheese", Category::Food),
    e("🌮", "taco", "mexican", Category::Food),
    e("🌯", "burrito", "wrap", Category::Food),
    e("🍜", "steaming bowl", "ramen noodles soup", Category::Food),
    e("🍣", "sushi", "japanese fish", Category::Food),
    e("🍪", "cookie", "sweet snack", Category::Food),
    e("🍩", "doughnut", "donut sweet", Category::Food),
    e("🍰", "shortcake", "cake birthday dessert", Category::Food),
    e("🍫", "chocolate bar", "sweet candy", Category::Food),
    e("🍿", "popcorn", "movie snack", Category::Food),
    e("☕", "hot beverage", "coffee tea cafe", Category::Food),
    e("🍵", "teacup without handle", "matcha green tea", Category::Food),
    e("🍺", "beer mug", "drink pub", Category::Food),
    e("🍻", "clinking beer mugs", "cheers drink", Category::Food),
    e("🍷", "wine glass", "drink", Category::Food),
    e("🥂", "clinking glasses", "cheers champagne", Category::Food),

    // ── Activities ───────────────────────────────────────────────────────────
    e("⚽", "soccer ball", "football sport", Category::Activities),
    e("🏀", "basketball", "sport", Category::Activities),
    e("🏈", "american football", "sport", Category::Activities),
    e("🎾", "tennis", "sport", Category::Activities),
    e("🏆", "trophy", "win award champion", Category::Activities),
    e("🥇", "1st place medal", "gold win", Category::Activities),
    e("🎮", "video game", "controller gaming", Category::Activities),
    e("🕹️", "joystick", "arcade game", Category::Activities),
    e("🎲", "game die", "dice random", Category::Activities),
    e("🎯", "bullseye", "target goal dart", Category::Activities),
    e("🎨", "artist palette", "art paint design", Category::Activities),
    e("🎭", "performing arts", "theater drama", Category::Activities),
    e("🎬", "clapper board", "movie film video", Category::Activities),
    e("🎤", "microphone", "sing karaoke audio", Category::Activities),
    e("🎧", "headphone", "music listen audio", Category::Activities),
    e("🎵", "musical note", "music song", Category::Activities),
    e("🎶", "musical notes", "music song", Category::Activities),
    e("🎸", "guitar", "music rock", Category::Activities),
    e("🎹", "musical keyboard", "piano music", Category::Activities),
    e("🎉", "party popper", "celebrate congrats yay", Category::Activities),
    e("🎊", "confetti ball", "party celebrate", Category::Activities),
    e("🎈", "balloon", "party birthday", Category::Activities),
    e("🎁", "wrapped gift", "present birthday", Category::Activities),

    // ── Travel & Places ──────────────────────────────────────────────────────
    e("🚗", "automobile", "car drive", Category::Travel),
    e("🚕", "taxi", "cab car", Category::Travel),
    e("🚌", "bus", "transit", Category::Travel),
    e("🚲", "bicycle", "bike cycle", Category::Travel),
    e("🚂", "locomotive", "train", Category::Travel),
    e("✈️", "airplane", "flight travel fly", Category::Travel),
    e("🚀", "rocket", "launch space ship fast", Category::Travel),
    e("🛸", "flying saucer", "ufo alien", Category::Travel),
    e("⛵", "sailboat", "boat sea", Category::Travel),
    e("🏠", "house", "home building", Category::Travel),
    e("🏢", "office building", "work", Category::Travel),
    e("🏔️", "snow-capped mountain", "mountain nature", Category::Travel),
    e("🏖️", "beach with umbrella", "vacation sea", Category::Travel),
    e("🏕️", "camping", "tent outdoors", Category::Travel),
    e("🗼", "Tokyo tower", "landmark japan", Category::Travel),
    e("🗽", "Statue of Liberty", "ny usa", Category::Travel),
    e("🗺️", "world map", "travel", Category::Travel),
    e("🧭", "compass", "direction navigate", Category::Travel),

    // ── Objects ──────────────────────────────────────────────────────────────
    e("💻", "laptop", "computer code pc", Category::Objects),
    e("🖥️", "desktop computer", "screen monitor pc", Category::Objects),
    e("⌨️", "keyboard", "type input", Category::Objects),
    e("🖱️", "computer mouse", "click", Category::Objects),
    e("📱", "mobile phone", "smartphone", Category::Objects),
    e("📷", "camera", "photo picture", Category::Objects),
    e("📺", "television", "tv screen", Category::Objects),
    e("🔋", "battery", "power energy", Category::Objects),
    e("🔌", "electric plug", "power charge", Category::Objects),
    e("💡", "light bulb", "idea tip bright", Category::Objects),
    e("🔦", "flashlight", "light", Category::Objects),
    e("📚", "books", "read study library", Category::Objects),
    e("📖", "open book", "read", Category::Objects),
    e("📝", "memo", "note write edit", Category::Objects),
    e("✏️", "pencil", "write draw", Category::Objects),
    e("📌", "pushpin", "pin note", Category::Objects),
    e("📎", "paperclip", "attach", Category::Objects),
    e("📁", "file folder", "directory", Category::Objects),
    e("📅", "calendar", "date schedule", Category::Objects),
    e("⏰", "alarm clock", "time wake", Category::Objects),
    e("⏳", "hourglass not done", "wait loading time", Category::Objects),
    e("🔍", "magnifying glass tilted left", "search find zoom", Category::Objects),
    e("🔑", "key", "lock password", Category::Objects),
    e("🔒", "locked", "security private", Category::Objects),
    e("🔓", "unlocked", "open security", Category::Objects),
    e("🔧", "wrench", "tool fix settings", Category::Objects),
    e("🔨", "hammer", "tool build", Category::Objects),
    e("⚙️", "gear", "settings config", Category::Objects),
    e("🧪", "test tube", "science lab experiment", Category::Objects),
    e("🧬", "dna", "science bio", Category::Objects),
    e("📦", "package", "box delivery crate", Category::Objects),
    e("🗑️", "wastebasket", "trash delete", Category::Objects),
    e("🔔", "bell", "notification alert", Category::Objects),
    e("🔕", "bell with slash", "mute dnd", Category::Objects),
    e("📣", "megaphone", "announce", Category::Objects),
    e("💎", "gem stone", "diamond ruby jewel", Category::Objects),
    e("💰", "money bag", "rich cash", Category::Objects),
    e("💳", "credit card", "pay buy", Category::Objects),

    // ── Symbols ──────────────────────────────────────────────────────────────
    e("❤️", "red heart", "love like", Category::Symbols),
    e("🧡", "orange heart", "love", Category::Symbols),
    e("💛", "yellow heart", "love", Category::Symbols),
    e("💚", "green heart", "love", Category::Symbols),
    e("💙", "blue heart", "love", Category::Symbols),
    e("💜", "purple heart", "love", Category::Symbols),
    e("🖤", "black heart", "love dark", Category::Symbols),
    e("🤍", "white heart", "love pure", Category::Symbols),
    e("💔", "broken heart", "sad", Category::Symbols),
    e("💯", "hundred points", "100 perfect score", Category::Symbols),
    e("💢", "anger symbol", "angry", Category::Symbols),
    e("💥", "collision", "boom bang", Category::Symbols),
    e("💫", "dizzy", "star", Category::Symbols),
    e("💬", "speech balloon", "chat message comment", Category::Symbols),
    e("💭", "thought balloon", "think", Category::Symbols),
    e("✅", "check mark button", "done yes ok pass", Category::Symbols),
    e("❌", "cross mark", "no wrong cancel fail", Category::Symbols),
    e("❗", "red exclamation mark", "alert important", Category::Symbols),
    e("❓", "red question mark", "what help", Category::Symbols),
    e("⚠️", "warning", "caution alert danger", Category::Symbols),
    e("🚫", "prohibited", "no ban stop", Category::Symbols),
    e("♻️", "recycling symbol", "recycle green", Category::Symbols),
    e("➕", "plus", "add", Category::Symbols),
    e("➖", "minus", "subtract", Category::Symbols),
    e("➡️", "right arrow", "next forward", Category::Symbols),
    e("⬅️", "left arrow", "back previous", Category::Symbols),
    e("⬆️", "up arrow", "top", Category::Symbols),
    e("⬇️", "down arrow", "bottom", Category::Symbols),
    e("🔄", "counterclockwise arrows", "refresh reload sync", Category::Symbols),
    e("♾️", "infinity", "forever loop", Category::Symbols),

    // ── Flags ────────────────────────────────────────────────────────────────
    e("🏁", "chequered flag", "race finish", Category::Flags),
    e("🚩", "triangular flag", "report red flag", Category::Flags),
    e("🏳️", "white flag", "surrender", Category::Flags),
    e("🏴", "black flag", "pirate", Category::Flags),
    e("🏳️‍🌈", "rainbow flag", "pride lgbtq", Category::Flags),
    e("🇺🇸", "flag: United States", "us usa america", Category::Flags),
    e("🇬🇧", "flag: United Kingdom", "uk gb britain", Category::Flags),
    e("🇫🇷", "flag: France", "fr french", Category::Flags),
    e("🇩🇪", "flag: Germany", "de german", Category::Flags),
    e("🇪🇸", "flag: Spain", "es spanish", Category::Flags),
    e("🇮🇹", "flag: Italy", "it italian", Category::Flags),
    e("🇯🇵", "flag: Japan", "jp japanese", Category::Flags),
    e("🇰🇷", "flag: South Korea", "kr korean", Category::Flags),
    e("🇨🇦", "flag: Canada", "ca canadian", Category::Flags),
    e("🇦🇺", "flag: Australia", "au", Category::Flags),
    e("🇧🇷", "flag: Brazil", "br", Category::Flags),
    e("🇮🇳", "flag: India", "in", Category::Flags),
    e("🇪🇺", "flag: European Union", "eu europe", Category::Flags),
];

pub struct EmojiPicker {
    clipboard: Entity<Clipboard>,
    query: String,
    category: Category,
    selected: usize,
    focus: FocusHandle,
    scroll: ScrollHandle,
}

impl EmojiPicker {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            clipboard: cx.service::<Clipboard>(),
            query: String::new(),
            category: Category::All,
            selected: 0,
            focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
        }
    }

    fn filtered(&self) -> Vec<&'static EmojiEntry> {
        let q = self.query.trim().to_lowercase();
        let terms: Vec<&str> = q.split_whitespace().collect();

        EMOJIS
            .iter()
            .filter(|entry| {
                if self.category != Category::All && entry.category != self.category {
                    return false;
                }
                if terms.is_empty() {
                    return true;
                }
                terms.iter().all(|t| {
                    entry.name.to_lowercase().contains(t)
                        || entry.keywords.to_lowercase().contains(t)
                        || entry.glyph.contains(t)
                })
            })
            .collect()
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.query.clear();
        self.selected = 0;
        self.scroll.scroll_to_item(0);
        cx.hide_window(ids::EMOJI);
        cx.notify();
    }

    fn activate(&mut self, i: usize, cx: &mut Context<Self>) {
        let items = self.filtered();
        if let Some(entry) = items.get(i) {
            let glyph = entry.glyph;
            self.clipboard.read(cx).copy_text(glyph);
        }
        self.close(cx);
    }

    fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        let n = self.filtered().len();
        if n > 0 {
            self.selected = (self.selected as isize + delta).rem_euclid(n as isize) as usize;
            self.scroll.scroll_to_item(self.selected / COLS);
            cx.notify();
        }
    }

    fn set_category(&mut self, cat: Category, cx: &mut Context<Self>) {
        self.category = cat;
        self.selected = 0;
        self.scroll.scroll_to_item(0);
        cx.notify();
    }

    fn on_key(&mut self, ev: &KeyDownEvent, cx: &mut Context<Self>) {
        match ui::key(ev) {
            "escape" => self.close(cx),
            "enter" => self.activate(self.selected, cx),
            "left" => self.step(-1, cx),
            "right" => self.step(1, cx),
            "up" => self.step(-(COLS as isize), cx),
            "down" => self.step(COLS as isize, cx),
            "tab" => {
                let next = if ev.keystroke.modifiers.shift {
                    self.category.prev()
                } else {
                    self.category.next()
                };
                self.set_category(next, cx);
            }
            "backspace" => {
                self.query.pop();
                self.selected = 0;
                self.scroll.scroll_to_item(0);
                cx.notify();
            }
            "space" => {
                self.query.push(' ');
                self.selected = 0;
                self.scroll.scroll_to_item(0);
                cx.notify();
            }
            _ => {
                let m = &ev.keystroke.modifiers;
                if !m.control && !m.alt && !m.platform {
                    if let Some(ch) = ev.keystroke.key_char.as_ref() {
                        if self.query.chars().count() < 64 {
                            self.query.push_str(ch);
                            self.selected = 0;
                            self.scroll.scroll_to_item(0);
                            cx.notify();
                        }
                    }
                }
            }
        }
    }
}

impl Render for EmojiPicker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.focus.is_focused(window) {
            window.focus(&self.focus, cx);
        }

        let items = self.filtered();
        let total = items.len();
        if self.selected >= total {
            self.selected = 0;
        }
        let scroll = self.scroll.clone();

        // ── Search bar ───────────────────────────────────────────────────────
        let input = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .px_4()
            .h(px(54.))
            .border_b_1()
            .border_color(pal::border())
            .child(ui::glyph(icons::SMILE, 16., pal::accent()))
            .child(if self.query.is_empty() {
                div()
                    .flex_1()
                    .text_color(pal::muted())
                    .child("Search emojis…")
            } else {
                div()
                    .flex_1()
                    .text_color(pal::text())
                    .child(format!("{}▏", self.query))
            })
            .child(
                div()
                    .text_xs()
                    .text_color(pal::muted())
                    .child(format!("{total}")),
            );

        // ── Category tabs ────────────────────────────────────────────────────
        let mut tabs = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap_1()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(pal::border());
        for (idx, cat) in Category::ALL_TABS.iter().copied().enumerate() {
            let active = self.category == cat;
            tabs = tabs.child(
                ui::btn(("emoji-cat", idx))
                    .h(px(26.))
                    .px_2()
                    .rounded(px(ui::RADIUS_CTRL))
                    .bg(if active {
                        pal::accent()
                    } else {
                        pal::with_alpha(pal::raised(), 0.0)
                    })
                    .hover(move |s| {
                        s.bg(if active {
                            pal::accent()
                        } else {
                            ui::hover_bg()
                        })
                    })
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.set_category(cat, cx);
                    }))
                    .child(
                        div()
                            .text_xs()
                            .font_weight(if active {
                                FontWeight::SEMIBOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .text_color(if active { pal::on_accent() } else { pal::muted() })
                            .child(cat.label()),
                    ),
            );
        }

        // ── Emoji grid (rows of COLS so scroll_to_item aligns with rows) ──────
        let mut grid = div()
            .id("emoji-scroll")
            .flex()
            .flex_col()
            .p_3()
            .gap_1()
            .overflow_y_scroll()
            .track_scroll(&scroll)
            .h(px(330.));

        if items.is_empty() {
            grid = grid.child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .py_8()
                    .text_color(pal::muted())
                    .child(format!("No emojis matching \"{}\".", self.query)),
            );
        } else {
            for (row_idx, chunk) in items.chunks(COLS).enumerate() {
                let mut row = div().flex().flex_row().gap_1();
                for (col_idx, entry) in chunk.iter().enumerate() {
                    let i = row_idx * COLS + col_idx;
                    let selected = i == self.selected;
                    row = row.child(
                        div()
                            .id(("emoji", i))
                            .w(px(54.))
                            .h(px(48.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(ui::RADIUS_CTRL))
                            .cursor_pointer()
                            .bg(if selected {
                                pal::raised()
                            } else {
                                pal::with_alpha(pal::raised(), 0.0)
                            })
                            .border_1()
                            .border_color(if selected {
                                pal::accent()
                            } else {
                                pal::with_alpha(pal::accent(), 0.0)
                            })
                            .hover(move |s| {
                                s.bg(if selected {
                                    pal::raised()
                                } else {
                                    ui::hover_bg()
                                })
                            })
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.activate(i, cx);
                            }))
                            .child(
                                div()
                                    .font_family(EMOJI_FONT)
                                    .text_size(px(24.))
                                    .child(display_glyph(entry.glyph)),
                            ),
                    );
                }
                grid = grid.child(row);
            }
        }

        // ── Footer preview ───────────────────────────────────────────────────
        let footer = {
            let current = items.get(self.selected).copied();
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .px_4()
                .h(px(46.))
                .border_t_1()
                .border_color(pal::border())
                .bg(pal::surface_a(0.6))
                .child(match current {
                    Some(entry) => div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_3()
                        .child(
                            div()
                                .font_family(EMOJI_FONT)
                                .text_size(px(22.))
                                .child(display_glyph(entry.glyph)),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(pal::text())
                                        .child(entry.name),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(pal::muted())
                                        .child(entry.category.label()),
                                ),
                        ),
                    None => div().text_xs().text_color(pal::muted()).child("No selection"),
                })
                .child(
                    div()
                        .text_xs()
                        .text_color(pal::muted())
                        .child("Enter copy · Tab category · Esc close"),
                )
        };

        ui::backdrop("emoji-bg", ids::EMOJI, ui::DIM_MODAL)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| this.on_key(ev, cx)))
            .flex()
            .justify_center()
            .items_start()
            .pt(px(140.))
            .child(
                div()
                    .occlude()
                    .w(px(604.))
                    .flex()
                    .flex_col()
                    .rounded(px(ui::RADIUS_PANEL))
                    .bg(pal::bg_a(0.97))
                    .border_1()
                    .border_color(pal::border())
                    .shadow_lg()
                    .overflow_hidden()
                    .child(input)
                    .child(tabs)
                    .child(grid)
                    .child(footer)
                    .with_animation("emoji-open", ui::anim_modal(), |el, delta| {
                        el.opacity(delta).mt(px(6.0 * (1.0 - delta)))
                    }),
            )
    }
}
