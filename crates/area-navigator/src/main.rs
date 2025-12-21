use iced::widget::{
    column, container, row, text, text_input, scrollable, button, image, svg, space,
    mouse_area,
};
use iced::{Alignment, Element, Length, Task, Theme, Color, window, Point};
use freedesktop_desktop_entry::{DesktopEntry, Iter as DesktopIter};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use linicon;
use xfce_rs_ui::styles;
use xfce_rs_ui::colors;

pub fn main() -> iced::Result {
    iced::application(Navigator::new, Navigator::update, Navigator::view)
        .title(Navigator::title)
        .theme(Navigator::theme)
        .style(Navigator::style)
        .window(iced::window::Settings {
            size: iced::Size::new(800.0, 600.0), // Increased size for new features
            position: iced::window::Position::Centered,
            transparent: true,
            decorations: false,
            ..Default::default()
        })
        .run()
}

struct Navigator {
    query: String,
    apps: Vec<AppEntry>,
    filtered_apps: Vec<AppEntry>,
    favorites: Vec<AppEntry>,
    suggestions: Vec<AppEntry>,
    maximized: bool,
    context_menu: Option<ContextMenu>,
    notification: Option<String>,
    last_mouse_pos: Point,
}

#[derive(Debug, Clone)]
struct ContextMenu {
    app: AppEntry,
    position: Point,
}

#[derive(Debug, Clone)]
enum Message {
    QueryChanged(String),
    LaunchApp(String),
    WindowDragged,
    Minimize,
    Maximize,
    Close,
    AddFavorite(AppEntry),
    CloseContextMenu,
    UninstallApp(AppEntry),
    ClearNotification,
    ShowMoreSuggestions,
    MouseMoved(Point),
    RightClickApp(AppEntry),
}

/// Represents the source of an icon to render differently in the view
#[derive(Debug, Clone, PartialEq, Eq)]
enum IconSource {
    Svg(PathBuf),
    Raster(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AppEntry {
    name: String,
    exec: String,
    id: String,
    icon: Option<IconSource>,
}

impl Navigator {
    fn new() -> (Self, Task<Message>) {
        let apps = scan_desktop_entries();
        let filtered_apps = apps.clone();
        
        // Mock favorites for now
        let favorites = apps.iter().take(5).cloned().collect();
        let suggestions = apps.iter().skip(10).take(6).cloned().collect();

        (
            Self {
                query: String::new(),
                apps,
                filtered_apps,
                favorites,
                suggestions,
                maximized: false,
                context_menu: None,
                notification: None,
                last_mouse_pos: Point::ORIGIN,
            },
            Task::none(),
        )
    }

    fn title(&self) -> String {
        String::from("Navigator")
    }

    fn theme(&self) -> Theme {
        Theme::Dark
    }

    fn style(&self, theme: &Theme) -> iced::theme::Style {
        iced::theme::Style {
            background_color: iced::Color::TRANSPARENT,
            text_color: theme.palette().text,
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::QueryChanged(new_query) => {
                self.query = new_query;
                if self.query.is_empty() {
                    self.filtered_apps = self.apps.clone();
                } else {
                    let matcher = SkimMatcherV2::default();
                    let mut scored: Vec<(i64, AppEntry)> = self.apps
                        .iter()
                        .filter_map(|app| {
                            matcher.fuzzy_match(&app.name, &self.query)
                                .map(|score| (score, app.clone()))
                        })
                        .collect();
                    scored.sort_by(|a, b| b.0.cmp(&a.0));
                    self.filtered_apps = scored.into_iter().map(|(_, app)| app).collect();
                }
                Task::none()
            }
            Message::LaunchApp(exec) => {
                let cleaned = exec
                    .replace("%f", "").replace("%F", "")
                    .replace("%u", "").replace("%U", "")
                    .trim().to_string();
                
                // Execute through shell to handle complex commands, environment variables, and shell syntax
                // Desktop entries often contain commands like "env VAR=value app" or shell constructs
                let result = StdCommand::new("sh")
                    .arg("-c")
                    .arg(&cleaned)
                    .spawn();
                
                match result {
                    Ok(_) => {
                        tracing::debug!("Successfully launched: {}", cleaned);
                    }
                    Err(e) => {
                        tracing::error!("Failed to launch '{}': {}", cleaned, e);
                        // Could show a notification to the user here
                    }
                }
                
                // Hide window instead of exiting to keep app alive
                window::latest().and_then(|id| window::minimize(id, true))
            }
            Message::WindowDragged => {
                 window::latest().and_then(|id| window::drag(id))
            }
            Message::Minimize => {
                window::latest().and_then(|id| window::minimize(id, true))
            }
            Message::Maximize => {
                self.maximized = !self.maximized;
                let maximized = self.maximized;
                window::latest().and_then(move |id| window::maximize(id, maximized))
            }
            Message::Close => {
                window::latest().and_then(|id| window::close(id))
            }
            Message::CloseContextMenu => {
                self.context_menu = None;
                Task::none()
            }
            Message::AddFavorite(app) => {
                if !self.favorites.iter().any(|f| f.id == app.id) {
                    self.favorites.push(app);
                }
                self.context_menu = None;
                Task::none()
            }
            Message::UninstallApp(app) => {
                self.notification = Some(format!("{} uninstalled successfully!", app.name));
                self.context_menu = None;
                Task::perform(tokio::time::sleep(tokio::time::Duration::from_secs(3)), |_| Message::ClearNotification)
            }
            Message::ClearNotification => {
                self.notification = None;
                Task::none()
            }
            Message::ShowMoreSuggestions => {
                // Just add 50 more random apps to suggestions
                let more: Vec<AppEntry> = self.apps.iter().skip(20).take(50).cloned().collect();
                self.suggestions.extend(more);
                Task::none()
            }
            Message::MouseMoved(p) => {
                self.last_mouse_pos = p;
                Task::none()
            }
            Message::RightClickApp(app) => {
                self.context_menu = Some(ContextMenu { app, position: self.last_mouse_pos });
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let logo_path = "crates/navigator/src/navigator-icon.svg";
        
        let header = row![
             // Buttons
             row![
                button(space().width(12).height(12))
                    .on_press(Message::Close)
                    .style(|theme, status| styles::window_control(theme, status, colors::CONTROL_CLOSE))
                    .width(12).height(12),
                button(space().width(12).height(12))
                     .on_press(Message::Minimize)
                     .style(|theme, status| styles::window_control(theme, status, colors::CONTROL_MIN))
                     .width(12).height(12),
                button(space().width(12).height(12))
                     .on_press(Message::Maximize)
                     .style(|theme, status| styles::window_control(theme, status, colors::CONTROL_MAX))
                     .width(12).height(12),
            ]
            .spacing(8)
            .padding(10),
            
            // Logo + Title
            row![
                svg(svg::Handle::from_path(logo_path)).width(20).height(20),
                text("Navigator").size(14).color(colors::TEXT_SECONDARY),
            ]
            .spacing(10)
            .width(Length::Fill)
            .align_y(Alignment::Center),
        ]
        .height(40)
        .align_y(Alignment::Center);

        // Favorites Bar
        let favorites_bar = container(
            row(self.favorites.iter().map(|app| {
                let icon: Element<Message> = match &app.icon {
                    Some(IconSource::Svg(path)) => svg(svg::Handle::from_path(path)).width(32).height(32).into(),
                    Some(IconSource::Raster(path)) => image(path).width(32).height(32).into(),
                    None => text("📦").size(32).into(),
                };
                
                button(icon)
                    .on_press(Message::LaunchApp(app.exec.clone()))
                    .padding(8)
                    .style(|theme, status| styles::app_card(theme, status))
                    .into()
            }))
            .spacing(15)
        )
        .padding(10)
        .width(Length::Fill)
        .center_x(Length::Fill);

        let input = text_input("Search applications...", &self.query)
            .on_input(Message::QueryChanged)
            .padding(15)
            .size(20)
            .style(|theme, status| styles::search_input(theme, status));

        // Suggestions
        let suggestions_section: Element<Message> = if self.query.is_empty() {
            column![
                row![
                    text("Suggestions").size(14).color(colors::TEXT_SECONDARY),
                    horizontal_space(),
                    button(text("search more -> shows more").size(12).color(colors::ACCENT_PRIMARY))
                        .on_press(Message::ShowMoreSuggestions)
                        .style(|_, _| button::Style { background: None, ..Default::default() }),
                ]
                .align_y(Alignment::Center)
                .padding(iced::Padding { top: 0.0, right: 0.0, bottom: 10.0, left: 0.0 }),

                scrollable(
                    row(self.suggestions.iter().map(|app| {
                        let icon: Element<Message> = match &app.icon {
                            Some(IconSource::Svg(path)) => svg(svg::Handle::from_path(path)).width(40).height(40).into(),
                            Some(IconSource::Raster(path)) => image(path).width(40).height(40).into(),
                            None => text("📦").size(40).into(),
                        };
                        
                        button(
                            column![
                                icon,
                                text(&app.name).size(12).color(Color::WHITE).width(60).align_x(Alignment::Center)
                            ]
                            .spacing(5)
                            .align_x(Alignment::Center)
                        )
                        .on_press(Message::LaunchApp(app.exec.clone()))
                        .padding(10)
                        .style(|theme, status| styles::app_card(theme, status))
                        .into()
                    }))
                    .spacing(20)
                ).direction(scrollable::Direction::Horizontal(scrollable::Scrollbar::default()))
            ]
            .spacing(5)
            .into()
        } else {
            column![].into()
        };

        let content = self.filtered_apps.iter().fold(
            column![].spacing(10).width(Length::Fill),
            |column, app| {
                let icon_widget: Element<Message> = match &app.icon {
                    Some(IconSource::Svg(path)) => svg(svg::Handle::from_path(path)).width(32).height(32).into(),
                    Some(IconSource::Raster(path)) => image(path).width(32).height(32).into(),
                    None => text("📦").size(32).into(),
                };

                // Track position for context menu
                let app_clone = app.clone();
                let entry = mouse_area(
                    button(
                        row![
                            icon_widget,
                            text(&app.name).size(18).color(Color::WHITE),
                        ]
                        .spacing(15)
                        .align_y(Alignment::Center),
                    )
                    .on_press(Message::LaunchApp(app.exec.clone()))
                    .width(Length::Fill)
                    .padding(12)
                    .style(|theme, status| styles::app_card(theme, status))
                )
                .on_move(Message::MouseMoved)
                .on_right_press(Message::RightClickApp(app_clone));

                column.push(entry)
            },
        );

        let main_content = column![
            header,
            favorites_bar,
            input,
            suggestions_section,
            scrollable(content).height(Length::Fill)
        ]
        .spacing(15)
        .padding(20);

        let mut layers = vec![
            // Layer 1: Base Glass
            container(space()).width(Length::Fill).height(Length::Fill).style(|theme| styles::glass_base(theme)).into(),
            // Layer 2: Edge Highlights (Boxed Gloss)
            container(space()).width(Length::Fill).height(Length::Fill).style(|theme| styles::glass_highlight_top(theme)).into(),
            container(space()).width(Length::Fill).height(Length::Fill).style(|theme| styles::glass_highlight_bottom(theme)).into(),
            container(space()).width(Length::Fill).height(Length::Fill).style(|theme| styles::glass_highlight_left(theme)).into(),
            container(space()).width(Length::Fill).height(Length::Fill).style(|theme| styles::glass_highlight_right(theme)).into(),

            // Layer 3: Global Drag Listener
            mouse_area(container(space()).width(Length::Fill).height(Length::Fill))
                .on_press(Message::WindowDragged).into(),

            // Layer 4: Content
            container(main_content).width(Length::Fill).height(Length::Fill).into(),
        ];

        // Layer 5: Context Menu
        if let Some(menu) = &self.context_menu {
            let menu_content = container(
                column![
                    button(text("Open").size(14))
                        .on_press(Message::LaunchApp(menu.app.exec.clone()))
                        .width(Length::Fill)
                        .padding(10)
                        .style(|theme, status| styles::app_card(theme, status)),
                    button(text("Add to Favorites").size(14))
                        .on_press(Message::AddFavorite(menu.app.clone()))
                        .width(Length::Fill)
                        .padding(10)
                        .style(|theme, status| styles::app_card(theme, status)),
                    button(text("Uninstall").size(14))
                        .on_press(Message::UninstallApp(menu.app.clone()))
                        .width(Length::Fill)
                        .padding(10)
                        .style(|theme, status| styles::app_card(theme, status)),
                ]
                .width(200)
            )
            .padding(5)
            .style(|theme| styles::glass_base(theme));

            layers.push(
                mouse_area(
                    container(
                        container(menu_content)
                            .padding(iced::Padding {
                                top: menu.position.y.max(0.0),
                                left: menu.position.x.max(0.0),
                                right: 0.0,
                                bottom: 0.0,
                            })
                    )
                    .width(Length::Fill)
                    .height(Length::Fill)
                )
                .on_press(Message::CloseContextMenu)
                .on_right_press(Message::CloseContextMenu)
                .into()
            );
        }

        // Layer 6: Notification
        if let Some(note) = &self.notification {
            layers.push(
                container(
                    container(text(note).color(Color::WHITE))
                        .padding(15)
                        .style(|theme| styles::glass_base(theme))
                )
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::End)
                .padding(40)
                .into()
            );
        }

        iced::widget::Stack::with_children(layers).into()
    }
}

fn horizontal_space() -> Element<'static, Message> {
    space().width(Length::Fill).into()
}

/// Resolves an icon source from a .desktop Icon key.
/// Follows the xfce4-panel fallback strategy:
/// 1. Absolute path -> use directly
/// 2. Icon theme lookup
/// 3. Strip extension and try icon theme again
/// 4. Look in /usr/share/pixmaps
fn resolve_icon(icon_key: &str) -> Option<IconSource> {
    let path = Path::new(icon_key);

    // 1. Check if it's an absolute path
    if path.is_absolute() && path.exists() {
        return path_to_icon_source(path);
    }

    // 2. Try linicon (icon theme lookup)
    if let Some(found) = linicon::lookup_icon(icon_key)
        .with_size(32)
        .next()
        .and_then(|r| r.ok())
    {
        return path_to_icon_source(&found.path);
    }

    // 3. Strip extension and try icon theme again (e.g., "app.png" -> "app")
    let name_without_ext = path.file_stem().and_then(|s| s.to_str()).unwrap_or(icon_key);
    if name_without_ext != icon_key {
        if let Some(found) = linicon::lookup_icon(name_without_ext)
            .with_size(32)
            .next()
            .and_then(|r| r.ok())
        {
            return path_to_icon_source(&found.path);
        }
    }

    // 4. Look in /usr/share/pixmaps
    for ext in &["svg", "png", "xpm"] {
        let pixmap_path = PathBuf::from(format!("/usr/share/pixmaps/{}.{}", icon_key, ext));
        if pixmap_path.exists() {
            return path_to_icon_source(&pixmap_path);
        }
    }

    None
}

fn path_to_icon_source(path: &Path) -> Option<IconSource> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    match ext.to_lowercase().as_str() {
        "svg" => Some(IconSource::Svg(path.to_path_buf())),
        "png" | "jpg" | "jpeg" | "xpm" => Some(IconSource::Raster(path.to_path_buf())),
        _ => None,
    }
}


fn scan_desktop_entries() -> Vec<AppEntry> {
    let mut entries = Vec::new();
    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/share:/usr/local/share".to_string());
    
    let mut search_paths: Vec<PathBuf> = data_dirs
        .split(':')
        .map(|p| PathBuf::from(p).join("applications"))
        .collect();

    if let Some(home) = dirs::home_dir() {
        search_paths.push(home.join(".local/share/applications"));
    }

    let locales: &[&str] = &["en_US", "en"];

    for entry_path in DesktopIter::new(search_paths.into_iter()) {
        if let Ok(bytes) = std::fs::read_to_string(&entry_path) {
            if let Ok(desktop) = DesktopEntry::from_str(&entry_path, &bytes, Some(locales)) {
                if desktop.no_display() || desktop.hidden() {
                    continue;
                }

                let exec = match desktop.exec() {
                    Some(e) => e.to_string(),
                    None => continue,
                };

                let id = entry_path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                let name = desktop.name(locales)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| id.clone());
                
                let icon = desktop.icon().and_then(resolve_icon);

                entries.push(AppEntry { name, exec, id, icon });
            }
        }
    }

    entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    entries
}
