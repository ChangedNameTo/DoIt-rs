use std::{collections::HashMap, time::Duration};

use clap::builder::Str;
use color_eyre::eyre::Result;
use crossterm::event::{KeyCode, KeyEvent};
use log::error;
use ratatui::{prelude::*, widgets::*};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;
use tui_input::{backend::crossterm::EventHandler, Input};

use super::{Component, Frame};
use crate::{
    action::Action,
    config::{Config, KeyBindings},
};

#[derive(Default, Clone)]
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
enum InputMode {
    #[default]
    Normal,
    Editing,
}

#[derive(Default)]
pub struct Home {
    command_tx: Option<UnboundedSender<Action>>,
    config: Config,
    todos: Vec<TodoItem>,
    input: Input,
    input_mode: InputMode,
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
            InputMode::Normal => return Ok(None),
            InputMode::Editing => match key.code {
                KeyCode::Esc => Action::ToggleCommandMode,
                KeyCode::Enter => {
                    if let Some(sender) = &self.command_tx {
                        if let Err(e) = sender.send(Action::AddTodo) {
                            error!("Failed to send action: {:?}", e);
                        }
                    }
                    Action::ToggleCommandMode
                }
                _ => {
                    self.input.handle_event(&crossterm::event::Event::Key(key));
                    Action::Refresh
                }
            },
        };
        Ok(Some(action))
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match self.input_mode {
            InputMode::Normal => match action {
                Action::ToggleCommandMode => {
                    self.input_mode = InputMode::Editing;
                }
                _ => {}
            },
            InputMode::Editing => match action {
                Action::ToggleCommandMode => {
                    self.input_mode = InputMode::Normal;
                }
                Action::AddTodo => {
                    let new_todo = TodoItem::new(self.input.value().into());
                    self.input.reset();
                    self.todos.push(new_todo);
                    self.input_mode = InputMode::Editing;
                }
                _ => {}
            },
        }
        Ok(None)
    }

    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(2)
            .constraints(
                [
                    Constraint::Length(1),
                    Constraint::Length(3),
                    Constraint::Min(1),
                ]
                .as_ref(),
            )
            .split(f.size());

        let (msg, style) = match self.input_mode {
            InputMode::Normal => (
                vec![
                    Span::raw("Press "),
                    Span::styled("CTRL+C", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(" to exit, "),
                    Span::styled("i", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(" to insert todo."),
                ],
                Style::default().add_modifier(Modifier::RAPID_BLINK),
            ),
            InputMode::Editing => (
                vec![
                    Span::raw("Press "),
                    Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(" to stop editing, "),
                    Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(" to record the todo"),
                ],
                Style::default(),
            ),
        };

        let mut text = Text::from(Line::from(msg));
        text.patch_style(style);
        let help_message = Paragraph::new(text);
        f.render_widget(help_message, chunks[0]);

        let width = chunks[0].width.max(3) - 3; // keep 2 for borders and 1 for cursor

        let scroll = self.input.visual_scroll(width as usize);
        let input = Paragraph::new(self.input.value())
            .style(match self.input_mode {
                InputMode::Normal => Style::default(),
                InputMode::Editing => Style::default().fg(Color::Yellow),
            })
            .scroll((0, scroll as u16))
            .block(Block::default().borders(Borders::ALL).title("Input"));
        f.render_widget(input, chunks[1]);

        match self.input_mode {
            InputMode::Normal =>
                // Hide the cursor. `Frame` does this by default, so we don't need to do anything here
                {}

            InputMode::Editing => {
                // Make the cursor visible and ask tui-rs to put it at the specified coordinates after rendering
                f.set_cursor(
                    // Put cursor past the end of the input text
                    chunks[0].x + ((self.input.visual_cursor()).max(scroll) - scroll) as u16 + 1,
                    // Move one line down, from the border to the input line
                    chunks[0].y + 2,
                )
            }
        }

        let todos: Vec<ListItem> = self
            .todos
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let content = vec![Line::from(Span::raw(format!("{}: {}", i, m.title)))];
                ListItem::new(content)
            })
            .collect();
        let todos = List::new(todos).block(Block::default().borders(Borders::ALL).title("Todo's"));
        f.render_widget(todos, chunks[2]);
        // Return OK
        Ok(())
    }
}
