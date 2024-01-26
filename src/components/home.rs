use std::{
    collections::HashMap,
    fmt::{self, write},
    fs::File,
    io::{BufWriter, Read, Write},
    time::Duration,
};

use clap::builder::Str;
use color_eyre::eyre::{Ok, Result};
use crossterm::event::{KeyCode, KeyEvent};
use log::*;
use ratatui::{prelude::*, widgets::*};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum::Display;
use tokio::sync::mpsc::UnboundedSender;
use tui_input::{backend::crossterm::EventHandler, Input};

use super::{Component, Frame};
use crate::{
    action::Action,
    config::{Config, KeyBindings},
    trace_dbg,
};

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    title: String,
}

impl TodoItem {
    pub fn new(title: String) -> Self {
        Self { title: title }
    }
}

impl Into<Text<'_>> for TodoItem {
    fn into(self) -> Text<'static> {
        Text::raw(self.title)
    }
}

#[derive(Default)]
enum Mode {
    #[default]
    Normal,
    Editing,
    Browse,
    Help,
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Mode::Normal => write!(f, "Normal"),
            Mode::Editing => write!(f, "Editing"),
            Mode::Browse => write!(f, "Browsing"),
            Mode::Help => write!(f, "Help"),
        }
    }
}

impl PartialEq for Mode {
    fn eq(&self, other: &Self) -> bool {
        core::mem::discriminant(self) == core::mem::discriminant(other)
    }
}

#[derive(Default)]
pub struct Home {
    command_tx: Option<UnboundedSender<Action>>,
    config: Config,
    todos: Vec<TodoItem>,
    input: Input,
    input_mode: Mode,
    cursor_row: i64,
}

impl Home {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Component for Home {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        self.command_tx = Some(tx);
        Ok(())
    }

    fn register_config_handler(&mut self, config: Config) -> Result<()> {
        self.config = config;
        Ok(())
    }

    fn handle_key_events(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        let action = match self.input_mode {
            Mode::Normal => match key.code {
                KeyCode::Char('i') => Action::EnterCommandMode,
                KeyCode::Char('v') => Action::EnterBrowseMode,
                KeyCode::Char('h') => Action::EnterHelpMode,
                _ => return Ok(None),
            },
            Mode::Editing => match key.code {
                KeyCode::Enter => {
                    if let Some(sender) = &self.command_tx {
                        if let Err(e) = sender.send(Action::AddTodo) {
                            error!("Failed to send action: {:?}", e);
                        }
                    }
                    Action::ExitCurrentMode
                }
                _ => {
                    self.input.handle_event(&crossterm::event::Event::Key(key));
                    Action::Refresh
                }
            },
            Mode::Browse => match key.code {
                KeyCode::Char('j') => Action::BrowseListDown,
                KeyCode::Char('k') => Action::BrowseListUp,
                _ => return Ok(None),
            },
            Mode::Help => match key.code {
                KeyCode::Char('h') => Action::ExitCurrentMode,
                _ => return Ok(None),
            },
        };
        Ok(Some(action))
    }

    fn buildup(&mut self) -> Result<()> {
        let file = File::open("./.data/home.json");

        match file {
            serde::__private::Ok(_) => {
                let mut buffer = String::new();
                file?.read_to_string(&mut buffer)?;
                let v: Vec<TodoItem> = serde_json::from_str(&buffer)?;

                if v.len() > 0 {
                    for todo_item in v.iter() {
                        let new_todo: TodoItem = TodoItem::new(todo_item.title.to_string());
                        self.todos.push(new_todo);
                    }
                }

                Ok(())
            }
            Err(_) => return Ok(()),
        }
    }

    fn teardown(&mut self) -> Result<()> {
        let file: File = File::create("./.data/home.json")?;
        let mut writer: BufWriter<File> = BufWriter::new(file);
        serde_json::to_writer(&mut writer, &self.todos)?;
        writer.flush()?;
        Ok(())
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match self.input_mode {
            Mode::Normal => match action {
                Action::EnterCommandMode => {
                    self.input_mode = Mode::Editing;
                }
                Action::EnterBrowseMode => {
                    self.input_mode = Mode::Browse;
                }
                Action::EnterHelpMode => {
                    self.input_mode = Mode::Help;
                }
                _ => {}
            },
            Mode::Editing => match action {
                Action::ExitCurrentMode => {
                    self.input_mode = Mode::Normal;
                }
                Action::AddTodo => {
                    let new_todo: TodoItem = TodoItem::new(self.input.value().into());
                    self.input.reset();
                    self.todos.push(new_todo);
                    self.input_mode = Mode::Editing;
                }
                _ => {}
            },
            Mode::Browse => match action {
                Action::ExitCurrentMode => {
                    self.input_mode = Mode::Normal;
                }
                Action::BrowseListUp => {
                    self.cursor_row -= 1;
                    self.cursor_row = self.cursor_row.max(0);
                }
                Action::BrowseListDown => {
                    self.cursor_row += 1;
                    self.cursor_row = self.cursor_row.min((self.todos.len() as i64) - 1);
                }
                _ => {}
            },
            Mode::Help => match action {
                Action::ExitCurrentMode => {
                    self.input_mode = Mode::Normal;
                }
                _ => {}
            },
        }
        Ok(None)
    }

    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        // Helper function for drawing the help box
        fn centered_rect(r: Rect, percent_x: u16, percent_y: u16) -> Rect {
            let popup_layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage((100 - percent_y) / 2),
                    Constraint::Percentage(percent_y),
                    Constraint::Percentage((100 - percent_y) / 2),
                ])
                .split(r);

            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage((100 - percent_x) / 2),
                    Constraint::Percentage(percent_x),
                    Constraint::Percentage((100 - percent_x) / 2),
                ])
                .split(popup_layout[1])[1]
        }

        if self.input_mode == Mode::Help {
            f.render_widget(
                Block::default().borders(Borders::all()).title("Help Menu"),
                centered_rect(f.size(), 35, 35),
            );
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(2)
            .constraints(
                [
                    Constraint::Min(5),
                    Constraint::Length(1),
                    Constraint::Length(3),
                    Constraint::Max(1),
                ]
                .as_ref(),
            )
            .split(f.size());

        let (msg, style) = match self.input_mode {
            Mode::Normal => (
                vec![
                    Span::raw("Press "),
                    Span::styled("CTRL+C", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(" to exit, "),
                    Span::styled("i", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(" to insert todo."),
                ],
                Style::default().add_modifier(Modifier::RAPID_BLINK),
            ),
            Mode::Editing => (
                vec![
                    Span::raw("Press "),
                    Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(" to stop editing, "),
                    Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(" to record the todo"),
                ],
                Style::default(),
            ),
            Mode::Browse => (
                vec![
                    Span::raw("Press "),
                    Span::styled("j", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(" to scroll down, "),
                    Span::styled("k", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(" to scroll up, "),
                    Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(" to exit browse mode "),
                ],
                Style::default(),
            ),
            Mode::Help => (vec![], Style::default()),
        };

        let mut text = Text::from(Line::from(msg));
        text.patch_style(style);
        let help_message = Paragraph::new(text);
        f.render_widget(help_message, chunks[1]);

        let width = chunks[1].width.max(3) - 3; // keep 2 for borders and 1 for cursor

        let scroll = self.input.visual_scroll(width as usize);
        let input = Paragraph::new(self.input.value())
            .style(match self.input_mode {
                Mode::Normal | Mode::Browse | Mode::Help => Style::default(),
                Mode::Editing => Style::default().fg(Color::Yellow),
            })
            .scroll((0, scroll as u16))
            .block(Block::default().borders(Borders::ALL).title("Input"));
        f.render_widget(input, chunks[2]);

        match self.input_mode {
            Mode::Normal | Mode::Browse | Mode::Help =>
                // Hide the cursor. `Frame` does this by default, so we don't need to do anything here
                {}

            Mode::Editing => {
                // Make the cursor visible and ask tui-rs to put it at the specified coordinates after rendering
                f.set_cursor(
                    // Put cursor past the end of the input text
                    chunks[2].x + ((self.input.visual_cursor()).max(scroll) - scroll) as u16 + 1,
                    // Move one line down, from the border to the input line
                    chunks[2].y + 1,
                )
            }
        }

        // Creates the todo list
        let todos: Vec<ListItem> = self
            .todos
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let content = vec![Line::from(Span::raw(format!("{}: {}", i, m.title)))];
                ListItem::new(content)
            })
            .collect();
        let todos = List::new(todos)
            .block(Block::default().borders(Borders::ALL).title("Todo's"))
            .highlight_style(Style::new().on_dark_gray())
            .highlight_spacing(HighlightSpacing::Always)
            .highlight_symbol(">>");
        let mut state = ListState::default();

        match self.input_mode {
            Mode::Editing | Mode::Normal | Mode::Help => {
                state.select(None);
            }
            Mode::Browse => {
                state.select(Some(self.cursor_row as usize));
            }
        }

        f.render_stateful_widget(todos, chunks[0], &mut state);

        let mode_indicator_text = self.input_mode.to_string();
        let mode_indicator_widget = Paragraph::new(Text::from(Line::from(mode_indicator_text)));
        f.render_widget(mode_indicator_widget, chunks[3]);

        // Return OK
        Ok(())
    }
}
