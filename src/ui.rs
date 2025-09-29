use cursive::event::Key;
use cursive::traits::*;
use cursive::views::{Dialog, LinearLayout, OnEventView, Panel, ScrollView, TextView};
use cursive::Cursive;
use cursive::CursiveExt;
use cursive::theme::{Theme, PaletteColor, Color, BorderStyle};
use std::sync::{Arc, Mutex};

fn render_list(items: &[String], selected: &[usize], cursor: usize) -> String {
    let mut out = String::new();
    out.push_str("Use Up/Down to move; Space to toggle (adds/removes order); Enter to confirm.\n\n");
    for (i, item) in items.iter().enumerate() {
        let pointer = if i == cursor { '>' } else { ' ' };
        let marker = match selected.iter().position(|&idx| idx == i) {
            Some(pos) => format!("[{:<2}]", pos + 1), // show 1-based order
            None => "[  ]".to_string(),
        };
        out.push_str(&format!("{} {} {}\n", pointer, marker, item));
    }
    out
}

pub fn select_ordered_with_cursive(items: Vec<String>) -> anyhow::Result<Vec<usize>> {
    use anyhow::anyhow;

    if items.is_empty() {
        return Err(anyhow!("No items to select"));
    }

    let mut siv = Cursive::default();

    // Apply a custom theme: black background with white text for best contrast.
    let mut theme = Theme::default();
    theme.shadow = false;
    theme.borders = BorderStyle::Simple;
    theme.palette[PaletteColor::Background] = Color::Rgb(0, 0, 0);
    theme.palette[PaletteColor::View] = Color::Rgb(0, 0, 0);
    // Primary text colors set to white
    theme.palette[PaletteColor::Primary] = Color::Rgb(255, 255, 255);
    theme.palette[PaletteColor::TitlePrimary] = Color::Rgb(255, 255, 255);
    theme.palette[PaletteColor::Highlight] = Color::Rgb(255, 255, 255);
    theme.palette[PaletteColor::HighlightInactive] = Color::Rgb(200, 200, 200);
    siv.set_theme(theme);

    #[derive(Clone)]
    struct State {
        items: Vec<String>,
        selected: Vec<usize>,
        cursor: usize,
    }

    let state = Arc::new(Mutex::new(State {
        items: items.clone(),
        selected: Vec::new(),
        cursor: 0,
    }));

    let initial = {
        let s = state.lock().unwrap();
        render_list(&s.items, &s.selected, s.cursor)
    };
    let text = TextView::new(initial).with_name("list");

    let st = state.clone();
    let mut view = OnEventView::new(text)
        .on_pre_event(Key::Up, move |siv| {
            let mut st = st.lock().unwrap();
            if st.cursor > 0 { st.cursor -= 1; }
            let content = render_list(&st.items, &st.selected, st.cursor);
            siv.call_on_name("list", |tv: &mut TextView| tv.set_content(content));
        });

    let st = state.clone();
    view.set_on_pre_event(Key::Down, move |siv| {
        let mut st = st.lock().unwrap();
        if st.cursor + 1 < st.items.len() { st.cursor += 1; }
        let content = render_list(&st.items, &st.selected, st.cursor);
        siv.call_on_name("list", |tv: &mut TextView| tv.set_content(content));
    });

    let st = state.clone();
    view.set_on_pre_event(' ', move |siv| {
        let mut st = st.lock().unwrap();
        let idx = st.cursor;
        if let Some(pos) = st.selected.iter().position(|&i| i == idx) {
            st.selected.remove(pos);
        } else {
            st.selected.push(idx);
        }
        let content = render_list(&st.items, &st.selected, st.cursor);
        siv.call_on_name("list", |tv: &mut TextView| tv.set_content(content));
    });

    let st = state.clone();
    view.set_on_pre_event(Key::Enter, move |siv| {
        let st = st.lock().unwrap();
        if st.selected.is_empty() {
            // ignore enter with no selections
            return;
        }
        siv.set_user_data(st.selected.clone());
        siv.quit();
    });

    // Build a compact, scrollable dialog instead of a fullscreen layer
    let root = Dialog::around(
        Panel::new(
            LinearLayout::vertical()
                .child(ScrollView::new(view).scroll_x(false).scroll_y(true))
        )
    )
    .title("Select tracks (Space to toggle, Enter to confirm)")
    .fixed_size((80, 20));

    siv.add_layer(root);

    siv.run();

    let result: Option<Vec<usize>> = siv.take_user_data();
    Ok(result.unwrap_or_default())
}

// New: inline, ordered selection using dialoguer::Select (no fullscreen alt-screen)
#[allow(dead_code)]
pub fn select_ordered_inline(items: Vec<String>) -> anyhow::Result<Vec<usize>> {
    use anyhow::anyhow;
    use dialoguer::{Select, theme::ColorfulTheme};

    if items.is_empty() {
        return Err(anyhow!("No items to select"));
    }

    // Keep original indices alongside display strings.
    let mut remaining: Vec<(usize, String)> = items.into_iter().enumerate().collect();
    let mut chosen: Vec<usize> = Vec::new();
    let mut chosen_labels: Vec<String> = Vec::new();

    // Repeatedly prompt until user finishes (via Done item or Esc) or nothing remains.
    loop {
        if remaining.is_empty() {
            break;
        }

        // Build a list with control items at the top.
        let mut menu: Vec<String> = Vec::new();
        menu.push("[Done] Finish selection".to_string());
        if let Some(last_label) = chosen_labels.last() {
            menu.push(format!("[Undo] Remove last pick: {}", last_label));
        }
        // Append remaining items
        for (_, s) in &remaining {
            menu.push(s.clone());
        }

        let picks_preview = if chosen_labels.is_empty() {
            "none".to_string()
        } else {
            chosen_labels.join(" | ")
        };
        let prompt = format!(
            "Pick next track (Enter). Use [Done] to finish, [Undo] to go back. Esc also finishes.\nCurrent picks: {}",
            picks_preview
        );

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt(prompt)
            .items(&menu)
            .default(0)
            .interact_opt()?;

        match selection {
            Some(0) => {
                // Done
                break;
            }
            Some(1) if !chosen.is_empty() => {
                // Undo last
                if let Some(last_idx) = chosen.pop() {
                    // Reinsert the item back to remaining at the end
                    // Find its label (from chosen_labels vec)
                    let last_label = chosen_labels.pop().unwrap_or_else(|| "(unknown)".to_string());
                    remaining.push((last_idx, last_label));
                }
            }
            Some(menu_idx) => {
                // Map to remaining index (subtract control items)
                let base = if chosen_labels.is_empty() { 1 } else { 2 };
                if menu_idx < base {
                    // Shouldn't happen, but guard anyway
                    continue;
                }
                let rem_idx = menu_idx - base;
                if rem_idx < remaining.len() {
                    let (orig_idx, label) = remaining.remove(rem_idx);
                    chosen.push(orig_idx);
                    chosen_labels.push(label);
                }
            }
            None => {
                // User pressed Esc/abort -> finish with whatever is selected so far
                break;
            }
        }
    }

    Ok(chosen)
}
