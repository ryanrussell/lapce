use std::sync::Arc;
use std::{collections::HashMap, path::Path};

use druid::menu::MenuEventCtx;
use druid::{
    piet::{Text, TextLayout as PietTextLayout, TextLayoutBuilder},
    BoxConstraints, Command, Cursor, Env, Event, EventCtx, LayoutCtx, LifeCycle,
    LifeCycleCtx, PaintCtx, Point, Rect, RenderContext, Size, Target, UpdateCtx,
    Widget, WidgetExt, WidgetId, WidgetPod,
};
use druid::{ExtEventSink, KbKey, WindowId};
use lapce_data::data::{LapceData, LapceEditorData};
use lapce_data::document::{BufferContent, LocalBufferKind};
use lapce_data::explorer::FileExplorerData;
use lapce_data::explorer::Naming;
use lapce_data::panel::PanelKind;
use lapce_data::proxy::LapceProxy;
use lapce_data::{
    command::LapceUICommand,
    command::LAPCE_UI_COMMAND,
    config::{Config, LapceTheme},
    data::LapceTabData,
};
use lapce_rpc::file::FileNodeItem;

use crate::editor::view::LapceEditorView;
use crate::{
    panel::{LapcePanel, PanelHeaderKind, PanelSizing},
    scroll::LapceScroll,
    svg::{file_svg, get_svg},
};

#[allow(clippy::too_many_arguments)]
/// Paint the file node item at its position
fn paint_single_file_node_item(
    ctx: &mut PaintCtx,
    item: &FileNodeItem,
    line_height: f64,
    width: f64,
    level: usize,
    current: usize,
    active: Option<&Path>,
    hovered: Option<usize>,
    config: &Config,
    toggle_rects: &mut HashMap<usize, Rect>,
) {
    let background = if Some(item.path_buf.as_ref()) == active {
        Some(LapceTheme::PANEL_CURRENT)
    } else if Some(current) == hovered {
        Some(LapceTheme::PANEL_HOVERED)
    } else {
        None
    };

    if let Some(background) = background {
        ctx.fill(
            Rect::ZERO
                .with_origin(Point::new(
                    0.0,
                    current as f64 * line_height - line_height,
                ))
                .with_size(Size::new(width, line_height)),
            config.get_color_unchecked(background),
        );
    }

    let y = current as f64 * line_height - line_height;
    let svg_y = y + 4.0;
    let svg_size = 15.0;
    let padding = 15.0 * level as f64;
    if item.is_dir {
        let icon_name = if item.open {
            "chevron-down.svg"
        } else {
            "chevron-right.svg"
        };
        let svg = get_svg(icon_name).unwrap();
        let rect = Size::new(svg_size, svg_size)
            .to_rect()
            .with_origin(Point::new(1.0 + padding, svg_y));
        ctx.draw_svg(
            &svg,
            rect,
            Some(config.get_color_unchecked(LapceTheme::EDITOR_FOREGROUND)),
        );
        toggle_rects.insert(current, rect);

        let icon_name = if item.open {
            "default_folder_opened.svg"
        } else {
            "default_folder.svg"
        };
        let svg = get_svg(icon_name).unwrap();
        let rect = Size::new(svg_size, svg_size)
            .to_rect()
            .with_origin(Point::new(1.0 + 16.0 + padding, svg_y));
        ctx.draw_svg(&svg, rect, None);
    } else {
        let (svg, svg_color) = file_svg(&item.path_buf);
        let rect = Size::new(svg_size, svg_size)
            .to_rect()
            .with_origin(Point::new(1.0 + 16.0 + padding, svg_y));
        ctx.draw_svg(&svg, rect, svg_color);
    }
    let text_layout = ctx
        .text()
        .new_text_layout(
            item.path_buf
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string(),
        )
        .font(config.ui.font_family(), config.ui.font_size() as f64)
        .text_color(
            config
                .get_color_unchecked(LapceTheme::EDITOR_FOREGROUND)
                .clone(),
        )
        .build()
        .unwrap();
    ctx.draw_text(
        &text_layout,
        Point::new(
            38.0 + padding,
            y + (line_height - text_layout.size().height) / 2.0,
        ),
    );
}

/// Paint the file node item, if it is in view, and its children
#[allow(clippy::too_many_arguments)]
pub fn paint_file_node_item(
    ctx: &mut PaintCtx,
    env: &Env,
    item: &FileNodeItem,
    min: usize,
    max: usize,
    line_height: f64,
    width: f64,
    level: usize,
    current: usize,
    active: Option<&Path>,
    hovered: Option<usize>,
    naming: Option<&Naming>,
    name_edit_input: &mut NameEditInput,
    drawn_name_input: &mut bool,
    data: &LapceTabData,
    config: &Config,
    toggle_rects: &mut HashMap<usize, Rect>,
) -> usize {
    if current > max {
        return current;
    }
    if current + item.children_open_count < min {
        return current + item.children_open_count;
    }

    let mut i = current;

    if current >= min {
        let mut should_paint_file_node = true;
        if !*drawn_name_input {
            if let Some(naming) = naming {
                if current == naming.list_index() {
                    draw_name_input(ctx, data, env, &mut i, naming, name_edit_input);
                    *drawn_name_input = true;
                    // If it is renaming then don't draw the underlying file node
                    should_paint_file_node =
                        !matches!(naming, Naming::Renaming { .. })
                }
            }
        }

        if should_paint_file_node {
            paint_single_file_node_item(
                ctx,
                item,
                line_height,
                width,
                level,
                i,
                active,
                hovered,
                config,
                toggle_rects,
            );
        }
    }

    if item.open {
        for item in item.sorted_children() {
            i = paint_file_node_item(
                ctx,
                env,
                item,
                min,
                max,
                line_height,
                width,
                level + 1,
                i + 1,
                active,
                hovered,
                naming,
                name_edit_input,
                drawn_name_input,
                data,
                config,
                toggle_rects,
            );
            if i > max {
                return i;
            }
        }
    }
    i
}

fn draw_name_input(
    ctx: &mut PaintCtx,
    data: &LapceTabData,
    env: &Env,
    i: &mut usize,
    naming: &Naming,
    name_edit_input: &mut NameEditInput,
) {
    match naming {
        Naming::Renaming { .. } => {
            name_edit_input.paint(ctx, data, env);
        }
        Naming::Naming { .. } => {
            name_edit_input.paint(ctx, data, env);
            // Skip forward by an entry
            // This is fine since we aren't using i as an index, but as an offset-multiple in painting
            *i += 1;
        }
    }
}

pub fn get_item_children(
    i: usize,
    index: usize,
    item: &FileNodeItem,
) -> (usize, Option<&FileNodeItem>) {
    if i == index {
        return (i, Some(item));
    }
    let mut i = i;
    if item.open {
        for child in item.sorted_children() {
            let count = child.children_open_count;
            if i + count + 1 >= index {
                let (new_index, node) = get_item_children(i + 1, index, child);
                if new_index == index {
                    return (new_index, node);
                }
            }
            i += count + 1;
        }
    }
    (i, None)
}

pub fn get_item_children_mut(
    i: usize,
    index: usize,
    item: &mut FileNodeItem,
) -> (usize, Option<&mut FileNodeItem>) {
    if i == index {
        return (i, Some(item));
    }
    let mut i = i;
    if item.open {
        for child in item.sorted_children_mut() {
            let count = child.children_open_count;
            if i + count + 1 >= index {
                let (new_index, node) = get_item_children_mut(i + 1, index, child);
                if new_index == index {
                    return (new_index, node);
                }
            }
            i += count + 1;
        }
    }
    (i, None)
}

pub struct FileExplorer {
    widget_id: WidgetId,
    file_list: WidgetPod<LapceTabData, Box<dyn Widget<LapceTabData>>>,
}

impl FileExplorer {
    pub fn new(data: &mut LapceTabData) -> Self {
        // Create the input editor for renaming/naming files/directories
        let editor = LapceEditorData::new(
            Some(data.file_explorer.renaming_editor_view_id),
            None,
            None,
            BufferContent::Local(LocalBufferKind::PathName),
            &data.config,
        );

        let input = LapceEditorView::new(editor.view_id, editor.editor_id, None)
            .hide_header()
            .hide_gutter()
            .hide_border()
            .set_background_color(LapceTheme::PANEL_HOVERED);
        let view_id = editor.view_id;
        data.main_split.editors.insert(view_id, Arc::new(editor));
        // Create the file listing
        let file_list = LapceScroll::new(FileExplorerFileList::new(WidgetPod::new(
            input.boxed(),
        )));

        Self {
            widget_id: data.file_explorer.widget_id,
            file_list: WidgetPod::new(file_list.boxed()),
        }
    }

    pub fn new_panel(data: &mut LapceTabData) -> LapcePanel {
        let split_id = WidgetId::next();
        LapcePanel::new(
            PanelKind::FileExplorer,
            data.file_explorer.widget_id,
            split_id,
            vec![(
                split_id,
                PanelHeaderKind::None,
                Self::new(data).boxed(),
                PanelSizing::Flex(false),
            )],
        )
    }
}

impl Widget<LapceTabData> for FileExplorer {
    fn id(&self) -> Option<WidgetId> {
        Some(self.widget_id)
    }

    fn event(
        &mut self,
        ctx: &mut EventCtx,
        event: &Event,
        data: &mut LapceTabData,
        env: &Env,
    ) {
        self.file_list.event(ctx, event, data, env);
    }

    fn lifecycle(
        &mut self,
        ctx: &mut LifeCycleCtx,
        event: &LifeCycle,
        data: &LapceTabData,
        env: &Env,
    ) {
        self.file_list.lifecycle(ctx, event, data, env);
    }

    fn update(
        &mut self,
        ctx: &mut UpdateCtx,
        _old_data: &LapceTabData,
        data: &LapceTabData,
        env: &Env,
    ) {
        self.file_list.update(ctx, data, env);
    }

    fn layout(
        &mut self,
        ctx: &mut LayoutCtx,
        bc: &BoxConstraints,
        data: &LapceTabData,
        env: &Env,
    ) -> Size {
        let self_size = bc.max();
        self.file_list.layout(ctx, bc, data, env);
        self.file_list
            .set_origin(ctx, data, env, Point::new(0.0, 0.0));
        self_size
    }

    fn paint(&mut self, ctx: &mut PaintCtx, data: &LapceTabData, env: &Env) {
        self.file_list.paint(ctx, data, env);
    }
}

type NameEditInput = WidgetPod<LapceTabData, Box<dyn Widget<LapceTabData>>>;
struct FileExplorerFileList {
    line_height: f64,
    hovered: Option<usize>,
    name_edit_input: NameEditInput,
}

impl FileExplorerFileList {
    pub fn new(input: NameEditInput) -> Self {
        Self {
            line_height: 25.0,
            hovered: None,
            name_edit_input: input,
        }
    }
}

impl Widget<LapceTabData> for FileExplorerFileList {
    fn event(
        &mut self,
        ctx: &mut EventCtx,
        event: &Event,
        data: &mut LapceTabData,
        env: &Env,
    ) {
        match event {
            Event::Command(cmd) if cmd.is(LAPCE_UI_COMMAND) => {
                let command = cmd.get_unchecked(LAPCE_UI_COMMAND);

                if let LapceUICommand::ActiveFileChanged { path } = command {
                    let file_explorer = Arc::make_mut(&mut data.file_explorer);
                    file_explorer.active_selected = path.clone();
                    ctx.request_paint();
                }
            }
            _ => {}
        }

        // Finish any renaming if the user presses enter
        if let Event::KeyDown(key_ev) = event {
            if self.name_edit_input.has_focus() {
                if key_ev.key == KbKey::Enter {
                    ctx.submit_command(Command::new(
                        LAPCE_UI_COMMAND,
                        LapceUICommand::ExplorerEndNaming { apply_naming: true },
                        Target::Auto,
                    ));
                } else if key_ev.key == KbKey::Escape {
                    ctx.submit_command(Command::new(
                        LAPCE_UI_COMMAND,
                        LapceUICommand::ExplorerEndNaming {
                            apply_naming: false,
                        },
                        Target::Auto,
                    ));
                }
            }
        }

        if data.file_explorer.naming.is_some() {
            self.name_edit_input.event(ctx, event, data, env);
            // If the input handled the event, then we just ignore it.
            if ctx.is_handled() {
                return;
            }
        }

        // We can catch these here because they'd be consumed by name edit input if they were for/on it
        if matches!(
            event,
            Event::MouseDown(_) | Event::KeyUp(_) | Event::KeyDown(_)
        ) && data.file_explorer.naming.is_some()
            && !self.name_edit_input.has_focus()
        {
            ctx.submit_command(Command::new(
                LAPCE_UI_COMMAND,
                LapceUICommand::ExplorerEndNaming { apply_naming: true },
                Target::Auto,
            ));
            return;
        }

        match event {
            Event::MouseMove(mouse_event) => {
                if !ctx.is_hot() {
                    return;
                }

                if let Some(workspace) = data.file_explorer.workspace.as_ref() {
                    let y = mouse_event.pos.y;
                    if y <= self.line_height
                        * (workspace.children_open_count + 1 + 1) as f64
                    {
                        ctx.set_cursor(&Cursor::Pointer);
                        let hovered = Some(
                            ((mouse_event.pos.y + self.line_height)
                                / self.line_height)
                                as usize,
                        );

                        if hovered != self.hovered {
                            ctx.request_paint();
                            self.hovered = hovered;
                        }
                    } else {
                        ctx.clear_cursor();
                        self.hovered = None;
                    }
                }
            }
            Event::MouseDown(mouse_event) => {
                if !ctx.is_hot() {
                    return;
                }

                let file_explorer = Arc::make_mut(&mut data.file_explorer);
                let index = ((mouse_event.pos.y + self.line_height)
                    / self.line_height) as usize;
                if mouse_event.button.is_left() {
                    if let Some((_, node)) =
                        file_explorer.get_node_by_index_mut(index)
                    {
                        if node.is_dir {
                            if node.read {
                                node.open = !node.open;
                            } else {
                                let tab_id = data.id;
                                let event_sink = ctx.get_external_handle();
                                FileExplorerData::read_dir(
                                    &node.path_buf,
                                    true,
                                    tab_id,
                                    &data.proxy,
                                    event_sink,
                                );
                            }
                            let path = node.path_buf.clone();
                            if let Some(paths) = file_explorer.node_tree(&path) {
                                for path in paths.iter() {
                                    file_explorer.update_node_count(path);
                                }
                            }
                        } else {
                            ctx.submit_command(Command::new(
                                LAPCE_UI_COMMAND,
                                LapceUICommand::OpenFile(node.path_buf.clone()),
                                Target::Widget(data.id),
                            ));
                            ctx.submit_command(Command::new(
                                LAPCE_UI_COMMAND,
                                LapceUICommand::ActiveFileChanged {
                                    path: Some(node.path_buf.clone()),
                                },
                                Target::Widget(file_explorer.widget_id),
                            ));
                        }
                    }
                }

                if mouse_event.button.is_right() {
                    if let Some((indent_level, node)) = file_explorer
                        .get_node_by_index(index)
                        .or_else(|| file_explorer.workspace.as_ref().map(|x| (0, x)))
                    {
                        let is_workspace = Some(&node.path_buf)
                            == file_explorer.workspace.as_ref().map(|x| &x.path_buf);

                        // The folder that it is, or is within
                        let base = if node.is_dir {
                            Some(node.path_buf.clone())
                        } else {
                            node.path_buf.parent().map(ToOwned::to_owned)
                        };

                        // If there's no reasonable path at the point, then ignore it
                        let base = if let Some(base) = base {
                            base
                        } else {
                            return;
                        };

                        // Create a context menu with different actions that can be performed on a file/dir
                        // or in the directory
                        let mut menu = druid::Menu::<LapceData>::new("Explorer");

                        // The ids are so that the correct LapceTabData can be acquired inside the menu event cb
                        // since the context menu only gets access to LapceData
                        let window_id = data.window_id;
                        let tab_id = data.id;
                        let item = druid::MenuItem::new("New File").on_activate(
                            make_new_file_cb(
                                ctx,
                                &base,
                                window_id,
                                tab_id,
                                is_workspace,
                                index,
                                indent_level,
                                false,
                            ),
                        );

                        menu = menu.entry(item);

                        let item = druid::MenuItem::new("New Directory")
                            .on_activate(make_new_file_cb(
                                ctx,
                                &base,
                                window_id,
                                tab_id,
                                is_workspace,
                                index,
                                indent_level,
                                true,
                            ));
                        menu = menu.entry(item);

                        // Separator between non destructive and destructive actions
                        menu = menu.separator();

                        // Don't allow us to rename or delete the current workspace
                        if !is_workspace {
                            let item = druid::MenuItem::new("Rename").command(
                                Command::new(
                                    LAPCE_UI_COMMAND,
                                    LapceUICommand::ExplorerStartRename {
                                        list_index: index,
                                        indent_level,
                                        text: node
                                            .path_buf
                                            .file_name()
                                            .map(|x| x.to_string_lossy().to_string())
                                            .unwrap_or_else(String::new),
                                    },
                                    Target::Auto,
                                ),
                            );
                            menu = menu.entry(item);

                            let trash_text = if node.is_dir {
                                "Move Directory to Trash"
                            } else {
                                "Move File to Trash"
                            };
                            let item = druid::MenuItem::new(trash_text).command(
                                Command::new(
                                    LAPCE_UI_COMMAND,
                                    LapceUICommand::TrashPath {
                                        path: node.path_buf.clone(),
                                    },
                                    Target::Auto,
                                ),
                            );
                            menu = menu.entry(item);
                        }

                        ctx.show_context_menu::<LapceData>(
                            menu,
                            ctx.to_window(mouse_event.pos),
                        );
                    }
                }
            }
            _ => {}
        }
    }

    fn lifecycle(
        &mut self,
        ctx: &mut LifeCycleCtx,
        event: &LifeCycle,
        data: &LapceTabData,
        env: &Env,
    ) {
        if let LifeCycle::HotChanged(false) = event {
            self.hovered = None;
        }

        self.name_edit_input.lifecycle(ctx, event, data, env);
    }

    fn update(
        &mut self,
        ctx: &mut UpdateCtx,
        old_data: &LapceTabData,
        data: &LapceTabData,
        env: &Env,
    ) {
        if data
            .file_explorer
            .workspace
            .as_ref()
            .map(|w| w.children_open_count)
            != old_data
                .file_explorer
                .workspace
                .as_ref()
                .map(|w| w.children_open_count)
        {
            ctx.request_layout();
        }

        if data.file_explorer.naming.is_some() {
            self.name_edit_input.update(ctx, data, env);
        }
    }

    fn layout(
        &mut self,
        ctx: &mut LayoutCtx,
        bc: &BoxConstraints,
        data: &LapceTabData,
        env: &Env,
    ) -> Size {
        if let Some(naming) = &data.file_explorer.naming {
            let (&index, &level) = match naming {
                Naming::Renaming {
                    list_index,
                    indent_level,
                }
                | Naming::Naming {
                    list_index,
                    indent_level,
                    ..
                } => (list_index, indent_level),
            };

            let max = bc.max();
            let input_bc = bc.shrink(Size::new(max.width / 2.0, 0.0));
            self.name_edit_input.layout(ctx, &input_bc, data, env);

            let y_pos = (index as f64 * self.line_height) - self.line_height;
            let x_pos = 38.0 + (15.0 * level as f64);
            self.name_edit_input.set_origin(
                ctx,
                data,
                env,
                Point::new(x_pos, y_pos),
            );
        }

        let mut height = data
            .file_explorer
            .workspace
            .as_ref()
            .map(|w| w.children_open_count)
            .unwrap_or(0);
        if matches!(data.file_explorer.naming, Some(Naming::Naming { .. })) {
            height += 1;
        }
        let height = height as f64 * self.line_height;
        // Choose whichever one is larger
        // We want to use bc.max().height when the number of entries is smaller than the window
        // height, because receiving right click events requires reporting that we fill the panel
        let height = height.max(bc.max().height);

        Size::new(bc.max().width, height)
    }

    fn paint(&mut self, ctx: &mut PaintCtx, data: &LapceTabData, env: &Env) {
        let rect = ctx.region().bounding_box();
        let size = ctx.size();
        let width = size.width;
        let active = data.file_explorer.active_selected.as_deref();
        let min = (rect.y0 / self.line_height).floor() as usize;
        let max = (rect.y1 / self.line_height) as usize + 2;
        let level = 0;
        let mut drawn_name_input = false;

        if let Some(item) = data.file_explorer.workspace.as_ref() {
            let mut i = 0;
            for item in item.sorted_children() {
                i = paint_file_node_item(
                    ctx,
                    env,
                    item,
                    min,
                    max,
                    self.line_height,
                    width,
                    level + 1,
                    i + 1,
                    active,
                    self.hovered,
                    data.file_explorer.naming.as_ref(),
                    &mut self.name_edit_input,
                    &mut drawn_name_input,
                    data,
                    &data.config,
                    &mut HashMap::new(),
                );
                if i > max {
                    return;
                }
            }

            // If we didn't draw the name input then we'll have to draw it here
            if let Some(naming) = &data.file_explorer.naming {
                if i == 0
                    || (naming.list_index() >= min && naming.list_index() < max)
                {
                    draw_name_input(
                        ctx,
                        data,
                        env,
                        // This value does not matter here
                        &mut 0,
                        naming,
                        &mut self.name_edit_input,
                    );
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
/// Create a callback for the context menu when creating a file/directory
/// This is the same function for both, besides one change in parameter
fn make_new_file_cb(
    ctx: &mut EventCtx,
    base: &Path,
    window_id: WindowId,
    tab_id: WidgetId,
    is_workspace: bool,
    index: usize,
    indent_level: usize,
    is_dir: bool,
) -> impl FnMut(&mut MenuEventCtx, &mut LapceData, &Env) + 'static {
    // If the node we're on is the workspace then we'll appear at the very start
    let display_index = if is_workspace { 1 } else { index + 1 };

    let event_sink = ctx.get_external_handle();
    let base_path = base.to_owned();
    move |_ctx, data: &mut LapceData, _env| {
        // Clone the handle within, since on_active is an FnMut, so we can't move it into the second
        // closure
        let event_sink = event_sink.clone();
        let base_path = base_path.clone();

        // Acquire the LapceTabData instance we were within
        let tab_data = data
            .windows
            .get_mut(&window_id)
            .unwrap()
            .tabs
            .get_mut(&tab_id)
            .unwrap();

        // Expand the directory, if it is one and if it needs to
        expand_dir(
            event_sink.clone(),
            &tab_data.proxy,
            tab_id,
            Arc::make_mut(&mut tab_data.file_explorer),
            index,
            move || {
                // After we send the command to update the directory, we submit the command to display the new file
                // input box
                // We ignore any error coming from submit command as failing here shouldn't crash lapce
                let res = event_sink.submit_command(
                    LAPCE_UI_COMMAND,
                    LapceUICommand::ExplorerNew {
                        list_index: display_index,
                        indent_level,
                        is_dir,
                        base_path,
                    },
                    Target::Auto,
                );

                if let Err(err) = res {
                    log::warn!(
                        "Failed to start constructing new/file directory: {:?}",
                        err
                    );
                }
            },
        );
    }
}

/// Expand the directory in the view
/// `on_finished` is called when its done, but the files in the list are not yet updated
/// but the command has been sent. This lets the user queue commands to occur right after it.
/// Note: `on_finished` is also called when there is no dir it didn't need reading
fn expand_dir(
    event_sink: ExtEventSink,
    proxy: &LapceProxy,
    tab_id: WidgetId,
    file_explorer: &mut FileExplorerData,
    index: usize,
    on_finished: impl FnOnce() + Send + 'static,
) {
    if let Some((_, node)) = file_explorer.get_node_by_index_mut(index) {
        if node.is_dir {
            if node.read {
                node.open = true;
                on_finished();
            } else {
                FileExplorerData::read_dir_cb(
                    &node.path_buf,
                    true,
                    tab_id,
                    proxy,
                    event_sink,
                    Some(on_finished),
                );
            }
            let path = node.path_buf.clone();
            if let Some(paths) = file_explorer.node_tree(&path) {
                for path in paths.iter() {
                    file_explorer.update_node_count(path);
                }
            }
        } else {
            on_finished();
        }
    } else {
        on_finished();
    }
}
