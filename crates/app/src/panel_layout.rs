use std::rc::Rc;

use cairn_core::settings::{PanelLayoutPreset, PanelLayouts};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::dock::{
    BasePanel, DockArea, DockAreaRenderer, DockAreaState, DockContext, DockEvent, DockPlacement,
    DockSkin, DockState, DragPanel, Panel, PanelEvent, PanelId, PanelInfo, PanelState, TabGroup,
    TabGroupRenderer, panel_handle, register_panel,
};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::{ActiveTheme, Sizable, WindowExt, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::repo_view::RepoView;
use crate::widgets::{icon, tr};

#[derive(Clone, Copy)]
pub(crate) enum LayoutPage {
    History,
    Changes,
}

impl LayoutPage {
    fn saved(self, layouts: &PanelLayouts) -> &Option<String> {
        match self {
            Self::History => &layouts.history,
            Self::Changes => &layouts.changes,
        }
    }

    fn set(self, layouts: &mut PanelLayouts, state: String) {
        match self {
            Self::History => layouts.history = Some(state),
            Self::Changes => layouts.changes = Some(state),
        }
    }

    fn factory(self, details_width: f32, list_width: f32) -> DockAreaState {
        let (panels, sizes) = match self {
            Self::History => (
                vec![Pane::History, Pane::Details],
                vec![px(0.), px(details_width.clamp(300., 900.))],
            ),
            Self::Changes => (
                vec![Pane::Files, Pane::Diff, Pane::Commit],
                vec![px(list_width.clamp(220., 640.)), px(0.), px(330.)],
            ),
        };
        let tabs = |pane: Pane| PanelState {
            panel_name: "TabPanel".into(),
            info: PanelInfo::tabs(0),
            children: vec![PanelState::new(pane.name())],
        };
        DockAreaState {
            version: Some(3),
            left_dock: Some(DockState::new(
                tabs(Pane::Navigation),
                DockPlacement::Left,
                px(260.),
                true,
            )),
            center: PanelState {
                panel_name: "StackPanel".into(),
                info: PanelInfo::stack(sizes, Axis::Horizontal),
                children: panels.into_iter().map(tabs).collect(),
            },
            ..Default::default()
        }
    }
}

#[derive(Clone, Copy)]
enum Pane {
    Navigation,
    History,
    Details,
    Files,
    Diff,
    Commit,
}

impl Pane {
    const ALL: [Self; 6] = [
        Self::Navigation,
        Self::History,
        Self::Details,
        Self::Files,
        Self::Diff,
        Self::Commit,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::Navigation => "cairn.navigation",
            Self::History => "cairn.history",
            Self::Details => "cairn.commit_details",
            Self::Files => "cairn.changed_files",
            Self::Diff => "cairn.diff",
            Self::Commit => "cairn.commit",
        }
    }

    fn title(self) -> SharedString {
        tr(match self {
            Self::Navigation => "panel_layout.navigation",
            Self::History => "page.history",
            Self::Details => "panel_layout.commit_details",
            Self::Files => "panel_layout.changed_files",
            Self::Diff => "panel_layout.diff",
            Self::Commit => "panel_layout.commit",
        })
    }
}

struct RepoPane {
    pane: Pane,
    repo: WeakEntity<RepoView>,
    app: WeakEntity<crate::app::CairnApp>,
    focus: FocusHandle,
    _repo_updates: Subscription,
    _app_updates: Option<Subscription>,
    group: Option<WeakEntity<TabGroup>>,
}

impl EventEmitter<PanelEvent> for RepoPane {}

impl Focusable for RepoPane {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl BasePanel for RepoPane {
    fn panel_name(&self) -> &'static str {
        self.pane.name()
    }

    fn closable(&self, _: &App) -> bool {
        false
    }

    fn on_added_to(&mut self, group: WeakEntity<TabGroup>, _: &mut Window, cx: &mut Context<Self>) {
        self.group = Some(group);
        cx.notify();
    }

    fn visible(&self, cx: &App) -> bool {
        !matches!(self.pane, Pane::Navigation)
            || self
                .app
                .upgrade()
                .is_some_and(|app| app.read(cx).sidebar_open)
    }
}

impl Panel for RepoPane {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.pane.title()
    }

    fn title_bar(&self, _: &App) -> bool {
        false
    }

    fn inner_padding(&self, _: &App) -> bool {
        false
    }
}

struct PanelDragPreview(SharedString);

impl Render for PanelDragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(cx.theme().background)
            .border_1()
            .border_color(cx.theme().border)
            .child(self.0.clone())
    }
}

impl Render for RepoPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = if matches!(self.pane, Pane::Navigation) {
            self.app
                .upgrade()
                .map(|app| app.update(cx, |app, cx| app.render_sidebar(cx).into_any_element()))
        } else {
            self.repo
                .update(cx, |repo, cx| match self.pane {
                    Pane::Navigation => unreachable!(),
                    Pane::History => repo.render_history_list(window, cx).into_any_element(),
                    Pane::Details => repo.render_commit_details(window, cx).into_any_element(),
                    Pane::Files => repo.render_file_list(cx).into_any_element(),
                    Pane::Diff => repo.render_changes_diff(cx).into_any_element(),
                    Pane::Commit => repo.render_commit_box(cx).into_any_element(),
                })
                .ok()
        };
        div()
            .size_full()
            .relative()
            .overflow_hidden()
            .track_focus(&self.focus)
            .when(!matches!(self.pane, Pane::History | Pane::Diff), |d| {
                d.bg(cx.theme().sidebar)
            })
            .children(content)
            .children(
                self.group
                    .as_ref()
                    .and_then(WeakEntity::upgrade)
                    .map(|group| {
                        let drag =
                            DragPanel::new(PanelId::from(cx.entity_id()), group.read(cx).node());
                        let label = self.pane.title();
                        div()
                            .id("panel-drag-edge")
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .h(px(7.))
                            .cursor_grab()
                            .on_drag(drag, move |drag, offset, _, cx| {
                                cx.stop_propagation();
                                drag.set_drag_offset(offset);
                                drag.set_preview_size(size(px(150.), px(28.)));
                                cx.new(|_| PanelDragPreview(label.clone()))
                            })
                    }),
            )
    }
}

// DockArea loads its panel tree synchronously. Its registry uses this binding
// to give every reconstructed panel the repository belonging to that area.
#[derive(Default)]
struct LoadingRepo(Option<(WeakEntity<RepoView>, WeakEntity<crate::app::CairnApp>)>);

impl Global for LoadingRepo {}

fn load_layout(
    area: &Entity<DockArea>,
    repo: WeakEntity<RepoView>,
    app: WeakEntity<crate::app::CairnApp>,
    state: DockAreaState,
    window: &mut Window,
    cx: &mut App,
) {
    if cx.try_global::<LoadingRepo>().is_none() {
        cx.set_global(LoadingRepo::default());
        for pane in Pane::ALL {
            register_panel(cx, pane.name(), move |_, _, cx| {
                let (repo, app) = cx
                    .global::<LoadingRepo>()
                    .0
                    .clone()
                    .expect("loading a repository layout");
                let entity = repo.upgrade().expect("loading an open repository layout");
                panel_handle(cx.new(|cx| {
                    RepoPane {
                        pane,
                        group: None,
                        repo,
                        app: app.clone(),
                        focus: cx.focus_handle(),
                        _repo_updates: cx.observe(&entity, |_, _, cx| cx.notify()),
                        _app_updates: app
                            .upgrade()
                            .filter(|_| matches!(pane, Pane::Navigation))
                            .map(|app| cx.observe(&app, |_, _, cx| cx.notify())),
                    }
                }))
            });
        }
    }
    cx.global_mut::<LoadingRepo>().0 = Some((repo, app));
    let result = area.update(cx, |area, cx| area.load(state, window, cx));
    cx.global_mut::<LoadingRepo>().0 = None;
    if let Err(error) = result {
        crate::repo_view::log_error(&format!("loading panel layout: {error}"));
    }
}

struct WorkspaceDock {
    skin: Rc<DockSkin>,
    repo: WeakEntity<RepoView>,
    area: WeakEntity<DockArea>,
}

impl DockAreaRenderer for WorkspaceDock {
    fn center_frame(&self, window: &mut Window, cx: &mut App) -> Stateful<Div> {
        let header = self
            .repo
            .update(cx, |repo, cx| {
                repo.render_toolbar(window, cx).into_any_element()
            })
            .ok();
        div().id("workspace-center").children(header)
    }

    fn render_dock(
        &self,
        dock: &DockContext,
        content: AnyElement,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let placement = dock.placement();
        if let Some(area) = self.area.upgrade() {
            window.defer(cx, move |window, cx| {
                area.update(cx, |area, cx| {
                    if area
                        .layout(placement)
                        .is_some_and(|tree| tree.panels().next().is_none())
                    {
                        area.remove_dock(placement, window, cx);
                    }
                });
            });
        }
        self.skin.render_dock(dock, content, window, cx)
    }

    fn tab_group_renderer(&self) -> Rc<dyn TabGroupRenderer> {
        self.skin.tab_group_renderer()
    }
}

struct ActiveLayout {
    area: Entity<DockArea>,
    applied: Option<String>,
    _changes: Subscription,
}

#[derive(Default)]
pub(crate) struct RepoPanelLayouts {
    history: Option<ActiveLayout>,
    changes: Option<ActiveLayout>,
}

impl RepoPanelLayouts {
    fn slot(&mut self, page: LayoutPage) -> &mut Option<ActiveLayout> {
        match page {
            LayoutPage::History => &mut self.history,
            LayoutPage::Changes => &mut self.changes,
        }
    }
}

impl RepoView {
    fn factory_panel_layout(&self, page: LayoutPage) -> DockAreaState {
        page.factory(
            self.settings.appearance.details_width,
            self.settings.appearance.list_width,
        )
    }

    fn ensure_panel_layout(
        &mut self,
        page: LayoutPage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<DockArea> {
        let saved = page.saved(&self.settings.panel_layouts).clone();
        if let Some(active) = self.panel_layouts.slot(page)
            && active.applied == saved
        {
            return active.area.clone();
        }
        let state = saved
            .as_deref()
            .and_then(|json| serde_json::from_str::<DockAreaState>(json).ok())
            .filter(|state| matches!(state.version, Some(2 | 3)))
            .unwrap_or_else(|| self.factory_panel_layout(page));
        let repo = cx.weak_entity();
        let area = cx.new(|cx| {
            let skin = DockSkin::new(cx);
            let renderer = Rc::new(WorkspaceDock {
                skin,
                repo,
                area: cx.weak_entity(),
            });
            DockArea::new("workspace-layout", Some(3), window, cx).with_renderer(renderer)
        });
        load_layout(&area, cx.weak_entity(), self.app.clone(), state, window, cx);
        let changes = cx.subscribe(&area, move |repo, area, event: &DockEvent, cx| {
            if !matches!(event, DockEvent::LayoutChanged) {
                return;
            }
            let Ok(state) = serde_json::to_string(&area.read(cx).dump(cx)) else {
                return;
            };
            let saved = page.saved(&repo.settings.panel_layouts);
            let Some(active) = repo.panel_layouts.slot(page) else {
                return;
            };
            if active.area != area || &active.applied != saved || saved.as_ref() == Some(&state) {
                return;
            }
            active.applied = Some(state.clone());
            repo.change_setting(move |s| page.set(&mut s.panel_layouts, state.clone()), cx);
        });
        *self.panel_layouts.slot(page) = Some(ActiveLayout {
            area: area.clone(),
            applied: saved,
            _changes: changes,
        });
        area
    }

    pub(crate) fn render_panel_layout(
        &mut self,
        page: LayoutPage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.ensure_panel_layout(page, window, cx)
            .into_any_element()
    }

    pub(crate) fn panel_layout_menu(
        &self,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let repo = cx.entity();
        Button::new("panel-layout")
            .small()
            .ghost()
            .icon(icon(IconName::LayoutGrid))
            .when(!compact, |button| button.label(tr("panel_layout.menu")))
            .tooltip(tr("panel_layout.menu"))
            .dropdown_menu(move |mut menu, _, cx| {
                let toggle = repo.clone();
                let enabled = repo.read(cx).settings.panel_layouts.enabled;
                let toggle_label = tr(if enabled {
                    "panel_layout.classic"
                } else {
                    "panel_layout.customize"
                });
                menu = menu
                    .item(PopupMenuItem::new(toggle_label).on_click(move |_, _, cx| {
                        toggle.update(cx, |repo, cx| {
                            repo.change_setting(
                                move |settings| settings.panel_layouts.enabled = !enabled,
                                cx,
                            );
                        });
                    }))
                    .separator();
                let save = repo.clone();
                let reset = repo.clone();
                menu = menu.item(PopupMenuItem::new(tr("panel_layout.save")).on_click(
                    move |_, window, cx| {
                        save_panel_layout(save.clone(), window, cx);
                    },
                ));
                let presets = &repo.read(cx).settings.panel_layouts.presets;
                if !presets.is_empty() {
                    menu = menu
                        .separator()
                        .item(PopupMenuItem::label(tr("panel_layout.presets")));
                }
                for (name, preset) in presets {
                    let repo = repo.clone();
                    let preset = preset.clone();
                    let delete = repo.clone();
                    let preset_name = name.clone();
                    let popup = cx.weak_entity();
                    let name = name.clone();
                    let item = PopupMenuItem::element(move |_, _| {
                        let delete = delete.clone();
                        let preset_name = preset_name.clone();
                        let popup = popup.clone();
                        h_flex()
                            .w_full()
                            .min_w_0()
                            .items_center()
                            .gap_2()
                            .child(div().flex_1().min_w_0().truncate().child(name.clone()))
                            .child(
                                Button::new(SharedString::from(format!("delete-preset-{preset_name}")))
                                    .xsmall()
                                    .ghost()
                                    .icon(icon(IconName::Trash).xsmall())
                                    .tooltip(tr("panel_layout.delete"))
                                    .on_click(move |_, window, cx| {
                                        cx.stop_propagation();
                                        let _ = popup.update(cx, |_, cx| cx.emit(DismissEvent));
                                        window.defer(cx, {
                                            let repo = delete.clone();
                                            let name = preset_name.clone();
                                            move |window, cx| {
                                                confirm_delete_preset(repo, name, window, cx)
                                            }
                                        });
                                    }),
                            )
                    })
                    .on_click(move |_, _, cx| {
                        repo.update(cx, |repo, cx| {
                            repo.change_setting(
                                {
                                    let preset = preset.clone();
                                    move |settings| {
                                        settings.panel_layouts.enabled = true;
                                        settings.panel_layouts.history =
                                            Some(preset.history.clone());
                                        settings.panel_layouts.changes =
                                            Some(preset.changes.clone());
                                    }
                                },
                                cx,
                            );
                        });
                    });
                    menu = menu.item(item);
                }
                menu.separator()
                    .item(
                        PopupMenuItem::new(tr("panel_layout.reset")).on_click(move |_, _, cx| {
                            reset.update(cx, |repo, cx| {
                                repo.panel_layouts = RepoPanelLayouts::default();
                                repo.change_setting(
                                    |settings| {
                                        settings.panel_layouts.enabled = false;
                                        settings.panel_layouts.history = None;
                                        settings.panel_layouts.changes = None;
                                        let factory = cairn_core::settings::Appearance::default();
                                        settings.appearance.details_width = factory.details_width;
                                        settings.appearance.list_width = factory.list_width;
                                    },
                                    cx,
                                );
                            });
                        }),
                    )
            })
    }
}

fn save_panel_layout(repo: Entity<RepoView>, window: &mut Window, cx: &mut App) {
    crate::dialogs::text_prompt(
        tr("panel_layout.save"),
        tr("panel_layout.save_description"),
        tr("panel_layout.name"),
        String::new(),
        tr("panel_layout.save"),
        window,
        cx,
        move |name, window, cx| {
            let preset = repo.update(cx, |repo, cx| {
                let history = repo.ensure_panel_layout(LayoutPage::History, window, cx);
                let changes = repo.ensure_panel_layout(LayoutPage::Changes, window, cx);
                Ok::<_, serde_json::Error>(PanelLayoutPreset {
                    history: serde_json::to_string(&history.read(cx).dump(cx))?,
                    changes: serde_json::to_string(&changes.read(cx).dump(cx))?,
                })
            });
            match preset {
                Ok(preset) => repo.update(cx, |repo, cx| {
                    repo.change_setting(
                        move |settings| {
                            settings
                                .panel_layouts
                                .presets
                                .insert(name.clone(), preset.clone());
                        },
                        cx,
                    );
                }),
                Err(error) => window.push_notification(
                    gpui_kit::component::notification::Notification::error(error.to_string()),
                    cx,
                ),
            }
        },
    );
}

fn confirm_delete_preset(repo: Entity<RepoView>, name: String, window: &mut Window, cx: &mut App) {
    window.open_dialog(cx, move |dialog, _, _| {
        let repo = repo.clone();
        let preset_name = name.clone();
        dialog
            .title(tr("panel_layout.delete"))
            .child(rust_i18n::t!("panel_layout.delete_confirm", name = name.clone()).to_string())
            .footer(crate::dialogs::footer(
                tr("panel_layout.delete_ok"),
                true,
                move |window, cx| {
                    let name = preset_name.clone();
                    repo.update(cx, |repo, cx| {
                        repo.change_setting(
                            move |settings| {
                                settings.panel_layouts.presets.remove(&name);
                            },
                            cx,
                        );
                    });
                    window.close_dialog(cx);
                },
            ))
    });
}
