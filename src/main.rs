use gpui::{
    App, Application, Bounds, Context, FocusHandle, Focusable, FontWeight, KeyBinding, Render,
    Window, WindowBounds, WindowOptions, actions, colors::Colors, div, prelude::*, px, size,
};
use serde::Deserialize;
use std::process::Command;

actions!(
    search_ui,
    [MoveUp, MoveDown, RunSearch, Backspace, ClearQuery, Close]
);

#[derive(Deserialize, Debug, Default)]
struct CliFile {
    path: String,
}

#[derive(Deserialize, Debug, Default)]
struct CliGrep {
    path: String,
    line: usize,
    text: String,
}

#[derive(Deserialize, Debug, Default)]
struct CliResponse {
    ok: bool,
    query: String,
    #[serde(default)]
    files: Vec<CliFile>,
    #[serde(default)]
    grep: Vec<CliGrep>,
    error: Option<String>,
}

#[derive(Clone, Copy)]
enum RowKind {
    File,
    Grep,
}

struct Row {
    kind: RowKind,
    title: String,
    subtitle: String,
}

struct SearchApp {
    focus_handle: FocusHandle,
    query: String,
    selected: usize,
    scroll_offset: usize,
    rows: Vec<Row>,
    status: String,
}

impl SearchApp {
    fn new(focus_handle: FocusHandle) -> Self {
        Self {
            focus_handle,
            query: "gpui".to_string(),
            selected: 0,
            scroll_offset: 0,
            rows: Vec::new(),
            status: "Type query and press Enter".to_string(),
        }
    }

    fn max_visible_rows(&self) -> usize {
        9
    }

    fn clamp_scroll_to_selected(&mut self) {
        let page = self.max_visible_rows();
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
            return;
        }

        let window_end = self.scroll_offset.saturating_add(page);
        if self.selected >= window_end {
            self.scroll_offset = self.selected.saturating_sub(page.saturating_sub(1));
        }
    }

    fn move_up(&mut self, _: &MoveUp, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected > 0 {
            self.selected -= 1;
            self.clamp_scroll_to_selected();
            cx.notify();
        }
    }

    fn move_down(&mut self, _: &MoveDown, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected + 1 < self.rows.len() {
            self.selected += 1;
            self.clamp_scroll_to_selected();
            cx.notify();
        }
    }

    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        self.query.pop();
        cx.notify();
    }

    fn clear_query(&mut self, _: &ClearQuery, _: &mut Window, cx: &mut Context<Self>) {
        self.query.clear();
        self.selected = 0;
        self.scroll_offset = 0;
        cx.notify();
    }

    fn push_char(&mut self, ch: char, cx: &mut Context<Self>) {
        self.query.push(ch);
        cx.notify();
    }

    fn run_search(&mut self, _: &RunSearch, _: &mut Window, cx: &mut Context<Self>) {
        if self.query.trim().is_empty() {
            self.rows.clear();
            self.selected = 0;
            self.scroll_offset = 0;
            self.status = "Empty query".to_string();
            cx.notify();
            return;
        }

        self.status = "Searching...".to_string();
        cx.notify();

        match run_bun_search(&self.query) {
            Ok(payload) => {
                if payload.ok {
                    self.rows.clear();
                    for file in payload.files {
                        self.rows.push(Row {
                            kind: RowKind::File,
                            title: file.path.clone(),
                            subtitle: "File match".to_string(),
                        });
                    }
                    for item in payload.grep {
                        self.rows.push(Row {
                            kind: RowKind::Grep,
                            title: format!("{}:{}", item.path, item.line),
                            subtitle: item.text,
                        });
                    }

                    self.selected = 0;
                    self.scroll_offset = 0;
                    self.status = format!(
                        "Query '{}' -> {} files, {} content matches",
                        payload.query,
                        self.rows
                            .iter()
                            .filter(|r| matches!(r.kind, RowKind::File))
                            .count(),
                        self.rows
                            .iter()
                            .filter(|r| matches!(r.kind, RowKind::Grep))
                            .count()
                    );
                } else {
                    self.rows.clear();
                    self.selected = 0;
                    self.scroll_offset = 0;
                    self.status = payload
                        .error
                        .unwrap_or_else(|| "Search failed with unknown error".to_string());
                }
            }
            Err(err) => {
                self.rows.clear();
                self.selected = 0;
                self.scroll_offset = 0;
                self.status = err;
            }
        }

        cx.notify();
    }
}

impl Focusable for SearchApp {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SearchApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = Colors::for_appearance(window);

        let results_list = if self.rows.is_empty() {
            div()
                .id("results")
                .flex_1()
                .rounded_lg()
                .border_1()
                .border_color(colors.border)
                .bg(colors.container)
                .overflow_scroll()
                .p_2()
                .child(
                    div()
                        .p_4()
                        .text_sm()
                        .text_color(colors.disabled)
                        .child("No results yet"),
                )
        } else {
            div()
                .id("results")
                .flex_1()
                .rounded_lg()
                .border_1()
                .border_color(colors.border)
                .bg(colors.container)
                .overflow_scroll()
                .p_2()
                .children(
                    self.rows
                        .iter()
                        .enumerate()
                        .skip(self.scroll_offset)
                        .take(self.max_visible_rows())
                        .map(|(index, row)| {
                            let active = index == self.selected;
                            row_view(row, active, &colors)
                        }),
                )
        };

        div()
            .id("root")
            .key_context("SearchApp")
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(Self::move_up))
            .on_action(cx.listener(Self::move_down))
            .on_action(cx.listener(Self::run_search))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::clear_query))
            .size_full()
            .p_4()
            .bg(colors.background)
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .h(px(48.))
                    .px_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(colors.border)
                    .bg(colors.container)
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors.text)
                            .child(self.query.clone()),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .bg(colors.separator)
                            .text_xs()
                            .text_color(colors.disabled)
                            .child("Enter"),
                    ),
            )
            .child(results_list)
            .child(
                div()
                    .h(px(30.))
                    .px_2()
                    .text_xs()
                    .text_color(colors.disabled)
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(self.status.clone())
                    .child("Arrows move | Ctrl+U clear | Esc quit"),
            )
    }
}

fn row_view(row: &Row, active: bool, colors: &Colors) -> impl IntoElement {
    let badge = match row.kind {
        RowKind::File => "FILE",
        RowKind::Grep => "GREP",
    };

    let active_bg: gpui::Hsla = colors.selected.into();

    div()
        .h(px(52.))
        .mb_1()
        .px_2()
        .rounded_md()
        .border_1()
        .border_color(if active {
            colors.selected
        } else {
            colors.border
        })
        .bg(if active {
            active_bg.opacity(0.2)
        } else {
            colors.background.into()
        })
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .w(px(48.))
                .px_1()
                .py_1()
                .rounded_sm()
                .bg(colors.separator)
                .text_xs()
                .text_color(colors.text)
                .child(badge),
        )
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(colors.text)
                        .child(row.title.clone()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(colors.disabled)
                        .child(row.subtitle.clone()),
                ),
        )
}

fn run_bun_search(query: &str) -> Result<CliResponse, String> {
    let output = Command::new("bun")
        .args(["index.ts", "--query", query, "--mode", "both"])
        .output()
        .map_err(|err| format!("Failed to run bun: {err}"))?;

    if output.stdout.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(if stderr.is_empty() {
            "bun returned no output".to_string()
        } else {
            stderr
        });
    }

    let payload: CliResponse = serde_json::from_slice(&output.stdout)
        .map_err(|err| format!("Bad JSON from bun: {err}"))?;

    if !output.status.success() && payload.ok {
        return Err("bun exited with non-zero status".to_string());
    }

    Ok(payload)
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(920.), px(620.)), cx);

        cx.bind_keys([
            KeyBinding::new("up", MoveUp, None),
            KeyBinding::new("down", MoveDown, None),
            KeyBinding::new("enter", RunSearch, None),
            KeyBinding::new("backspace", Backspace, None),
            KeyBinding::new("ctrl-u", ClearQuery, None),
            KeyBinding::new("escape", Close, None),
        ]);

        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(gpui::TitlebarOptions {
                        title: Some("the-search-thing 2".into()),
                        appears_transparent: true,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |_, cx| cx.new(|cx| SearchApp::new(cx.focus_handle())),
            )
            .expect("Failed to open window");

        let view = window.update(cx, |_, _, cx| cx.entity()).expect("entity");

        window
            .update(cx, |view, window, cx| {
                window.focus(&view.focus_handle(cx));
                cx.activate(true);
            })
            .expect("focus");

        cx.observe_keystrokes(move |ev, _, cx| {
            if ev.keystroke.modifiers.control
                || ev.keystroke.modifiers.alt
                || ev.keystroke.modifiers.platform
            {
                return;
            }

            if let Some(key_char) = ev.keystroke.key_char.as_ref()
                && key_char.chars().count() == 1
            {
                let ch = key_char.chars().next().unwrap_or_default();
                if !ch.is_control() {
                    let _ = view.update(cx, |view, cx| view.push_char(ch, cx));
                }
            }
        })
        .detach();

        cx.on_action(|_: &Close, cx| cx.quit());
    });
}
