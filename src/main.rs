#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod qr;
mod server;
mod store;

use std::net::IpAddr;
use std::path::PathBuf;

use gpui_kit::component::button::*;
use gpui_kit::component::dialog::{DialogClose, DialogFooter};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _, Icon, IconName, Sizable as _, StyledExt as _, TitleBar,
    WindowExt as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use server::ServerHandle;
use store::SiteConfig;

const BASE_PORT: u16 = 8080;

struct Site {
    cfg: SiteConfig,
    handle: Option<ServerHandle>,
}

impl Site {
    fn running(&self) -> bool {
        self.handle.is_some()
    }
}

struct HttpServerApp {
    sites: Vec<Site>,
    next_id: u64,
    ip: IpAddr,
    grid: bool,
}

impl HttpServerApp {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let configs = store::load();
        let next_id = configs.iter().map(|c| c.id).max().unwrap_or(0) + 1;
        let mut this = Self {
            sites: configs.into_iter().map(|cfg| Site { cfg, handle: None }).collect(),
            next_id,
            ip: server::lan_ip(),
            grid: store::load_grid(),
        };
        // Restore sites that were running last time.
        for idx in 0..this.sites.len() {
            if this.sites[idx].cfg.running {
                if let Err(err) = this.start(idx) {
                    window.push_notification(Notification::error(err.to_string()), cx);
                }
            }
        }
        this.persist();
        this
    }

    fn url(&self, site: &Site) -> String {
        format!("http://{}:{}", self.ip, site.cfg.port)
    }

    fn persist(&self) {
        let configs: Vec<_> = self.sites.iter().map(|s| s.cfg.clone()).collect();
        store::save(&configs);
    }

    fn taken_ports(&self, except: Option<usize>) -> Vec<u16> {
        self.sites
            .iter()
            .enumerate()
            .filter(|(i, _)| Some(*i) != except)
            .map(|(_, s)| s.cfg.port)
            .collect()
    }

    fn start(&mut self, idx: usize) -> anyhow::Result<()> {
        // Keep the remembered port when it is still free, otherwise pick another.
        let taken = self.taken_ports(Some(idx));
        let site = &mut self.sites[idx];
        let port = site.cfg.port;
        if taken.contains(&port) || !server::port_is_free(port) {
            site.cfg.port = server::find_free_port(BASE_PORT, &taken);
        }
        site.handle = Some(ServerHandle::start(site.cfg.path.clone(), site.cfg.port)?);
        site.cfg.running = true;
        Ok(())
    }

    fn stop(&mut self, idx: usize) {
        let site = &mut self.sites[idx];
        site.handle = None; // Drop stops the listener.
        site.cfg.running = false;
    }

    fn add_paths(&mut self, paths: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        let mut added = 0;
        for path in paths {
            let dir = if path.is_dir() { path } else { path.parent().map(PathBuf::from).unwrap_or(path) };
            if !dir.is_dir() || self.sites.iter().any(|s| s.cfg.path == dir) {
                continue;
            }
            let taken = self.taken_ports(None);
            let cfg = SiteConfig {
                id: self.next_id,
                path: dir,
                port: server::find_free_port(BASE_PORT, &taken),
                running: false,
                custom_name: None,
            };
            self.next_id += 1;
            self.sites.push(Site { cfg, handle: None });
            let idx = self.sites.len() - 1;
            if let Err(err) = self.start(idx) {
                window.push_notification(Notification::error(err.to_string()), cx);
            }
            added += 1;
        }
        if added == 0 {
            window.push_notification("文件夹已在列表中", cx);
        }
        self.persist();
        cx.notify();
    }

    fn pick_folders(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: true,
            prompt: Some("添加".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                this.update_in(cx, |this, window, cx| this.add_paths(paths, window, cx)).ok();
            }
        })
        .detach();
    }

    fn set_running(&mut self, idx: usize, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        if on {
            if let Err(err) = self.start(idx) {
                window.push_notification(Notification::error(err.to_string()), cx);
            }
        } else {
            self.stop(idx);
        }
        self.persist();
        cx.notify();
    }

    fn confirm_remove(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(site) = self.sites.iter().find(|s| s.cfg.id == id) else { return };
        let title = format!("移除「{}」？", site.cfg.name());
        let this = cx.entity();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let this = this.clone();
            alert
                .title(title.clone())
                .description("只会从列表中移除并停止服务，磁盘上的文件不会被删除。")
                .confirm()
                .ok_text("移除")
                .ok_variant(ButtonVariant::Danger)
                .on_ok(move |_, _, cx| {
                    this.update(cx, |app, cx| {
                        app.sites.retain(|s| s.cfg.id != id);
                        app.persist();
                        cx.notify();
                    });
                    true
                })
        });
    }

    fn rename(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(site) = self.sites.iter().find(|s| s.cfg.id == id) else { return };
        let current = site.cfg.name();
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("项目名称").default_value(current));
        let this = cx.entity();
        window.open_dialog(cx, move |dialog, _, _| {
            let (input, this) = (input.clone(), this.clone());
            dialog
                .title("重命名项目")
                .w(px(380.))
                .child(Input::new(&input))
                .footer(
                    DialogFooter::new()
                        .gap_2()
                        .child(DialogClose::new().child(Button::new("rename-cancel").label("取消").outline()))
                        .child(DialogClose::new().child(
                            Button::new("rename-ok").primary().label("保存").on_click({
                                let input = input.clone();
                                move |_, _, cx| {
                                    let value = input.read(cx).value().trim().to_string();
                                    this.update(cx, |app, cx| {
                                        if let Some(s) = app.sites.iter_mut().find(|s| s.cfg.id == id) {
                                            s.cfg.custom_name = (!value.is_empty()).then_some(value);
                                        }
                                        app.persist();
                                        cx.notify();
                                    });
                                }
                            }),
                        )),
                )
        });
    }

    fn show_qr(&mut self, idx: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.ip = server::lan_ip();
        let site = &self.sites[idx];
        let url = self.url(site);
        let name = site.cfg.name();
        window.open_dialog(cx, move |dialog, _, cx| {
            let (copy_url, open_url) = (url.clone(), url.clone());
            dialog
                .title(name.clone())
                .w(px(380.))
                .child(
                    v_flex()
                        .items_center()
                        .gap_4()
                        .pt_2()
                        .child(qr::qr_tile(&url, 220.))
                        .child(
                            v_flex()
                                .items_center()
                                .gap_1()
                                .child(div().font_semibold().child(url.clone()))
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("用手机扫码访问，需与电脑连接同一 Wi‑Fi"),
                                ),
                        ),
                )
                .footer(
                    DialogFooter::new()
                        .gap_2()
                        .child(
                            Button::new("qr-copy")
                                .icon(IconName::Copy)
                                .label("复制链接")
                                .on_click(move |_, window, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(copy_url.clone()));
                                    window.push_notification("链接已复制", cx);
                                }),
                        )
                        .child(DialogClose::new().child(
                            Button::new("qr-open")
                                .primary()
                                .icon(IconName::ExternalLink)
                                .label("在浏览器打开")
                                .on_click(move |_, _, cx| cx.open_url(&open_url)),
                        )),
                )
        });
    }

    fn render_empty(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_4()
            .child(
                div()
                    .size(px(72.))
                    .rounded_full()
                    .bg(cx.theme().muted)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Icon::new(IconName::Inbox).size_8().text_color(cx.theme().muted_foreground)),
            )
            .child(
                v_flex()
                    .items_center()
                    .gap_1()
                    .child(div().text_xl().font_semibold().child("添加一个静态网站"))
                    .child(
                        div()
                            .text_color(cx.theme().muted_foreground)
                            .child("选择包含 HTML 的文件夹，或直接拖到这里"),
                    ),
            )
            .child(
                Button::new("empty-add")
                    .primary()
                    .icon(IconName::Plus)
                    .label("添加文件夹")
                    .on_click(cx.listener(|this, _, window, cx| this.pick_folders(window, cx))),
            )
    }

    fn render_card(&self, idx: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let site = &self.sites[idx];
        let id = site.cfg.id;
        let running = site.running();
        let url = self.url(site);
        let theme = cx.theme().clone();
        let path = site.cfg.path.clone();

        let subtitle = if running {
            h_flex()
                .gap_1()
                .items_center()
                .child(div().flex_1().min_w_0().truncate().text_sm().text_color(theme.primary).child(url.clone()))
                .child({
                    let url = url.clone();
                    Button::new(("gcopy", id)).ghost().xsmall().icon(IconName::Copy).tooltip("复制链接").on_click(
                        move |_, window, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(url.clone()));
                            window.push_notification("链接已复制", cx);
                        },
                    )
                })
                .into_any_element()
        } else {
            div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(format!("已停止 · 端口 {}", site.cfg.port))
                .into_any_element()
        };

        v_flex()
            .id(("card", id))
            .w(px(260.))
            .gap_3()
            .p_4()
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            .bg(theme.background)
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(div().size(px(8.)).flex_none().rounded_full().bg(if running {
                        theme.success
                    } else {
                        theme.muted_foreground.opacity(0.4)
                    }))
                    .child(div().flex_1().min_w_0().font_semibold().truncate().child(site.cfg.name()))
                    .child(Switch::new(("gswitch", id)).checked(running).on_change(cx.listener(
                        move |this, on: &bool, window, cx| this.set_running(idx, *on, window, cx),
                    ))),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_xs().text_color(theme.muted_foreground).truncate().child(path.display().to_string()))
                    .child(subtitle),
            )
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(
                        Button::new(("gqr", id))
                            .outline()
                            .small()
                            .label("二维码")
                            .disabled(!running)
                            .on_click(cx.listener(move |this, _, window, cx| this.show_qr(idx, window, cx))),
                    )
                    .child({
                        let url = url.clone();
                        Button::new(("gopen", id))
                            .ghost()
                            .icon(IconName::Globe)
                            .tooltip("在浏览器打开")
                            .disabled(!running)
                            .on_click(move |_, _, cx| cx.open_url(&url))
                    })
                    .child(
                        Button::new(("greveal", id))
                            .ghost()
                            .icon(IconName::FolderOpen)
                            .tooltip("在 Finder 中显示")
                            .on_click(move |_, _, cx| cx.reveal_path(&path)),
                    )
                    .child(
                        Button::new(("grename", id))
                            .ghost()
                            .small()
                            .label("重命名")
                            .on_click(cx.listener(move |this, _, window, cx| this.rename(id, window, cx))),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new(("gremove", id))
                            .ghost()
                            .icon(IconName::Close)
                            .tooltip("移除")
                            .on_click(cx.listener(move |this, _, window, cx| this.confirm_remove(id, window, cx))),
                    ),
            )
    }

    fn render_site(&self, idx: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let site = &self.sites[idx];
        let id = site.cfg.id;
        let running = site.running();
        let url = self.url(site);
        let theme = cx.theme().clone();
        let path = site.cfg.path.clone();

        let subtitle = if running {
            h_flex()
                .gap_1()
                .items_center()
                .child(div().text_sm().text_color(theme.primary).child(url.clone()))
                .child({
                    let url = url.clone();
                    Button::new(("copy", id))
                        .ghost()
                        .xsmall()
                        .icon(IconName::Copy)
                        .tooltip("复制链接")
                        .on_click(move |_, window, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(url.clone()));
                            window.push_notification("链接已复制", cx);
                        })
                })
                .into_any_element()
        } else {
            div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(format!("已停止 · 端口 {}", site.cfg.port))
                .into_any_element()
        };

        h_flex()
            .id(("site", id))
            .gap_4()
            .p_4()
            .items_center()
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            .bg(theme.background)
            .child(
                div()
                    .size(px(8.))
                    .flex_none()
                    .rounded_full()
                    .bg(if running { theme.success } else { theme.muted_foreground.opacity(0.4) }),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(
                        h_flex()
                            .gap_1()
                            .items_center()
                            .child(div().font_semibold().truncate().child(site.cfg.name()))
                            .child(
                                Button::new(("rename", id))
                                    .ghost()
                                    .xsmall()
                                    .label("重命名")
                                    .on_click(cx.listener(move |this, _, window, cx| this.rename(id, window, cx))),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .truncate()
                            .child(path.display().to_string()),
                    )
                    .child(subtitle),
            )
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(
                        Button::new(("qr", id))
                            .ghost()
                            .outline()
                            .small()
                            .label("二维码")
                            .tooltip("手机扫码访问")
                            .disabled(!running)
                            .on_click(cx.listener(move |this, _, window, cx| this.show_qr(idx, window, cx))),
                    )
                    .child({
                        let url = url.clone();
                        Button::new(("open", id))
                            .ghost()
                            .icon(IconName::Globe)
                            .tooltip("在浏览器打开")
                            .disabled(!running)
                            .on_click(move |_, _, cx| cx.open_url(&url))
                    })
                    .child(
                        Button::new(("reveal", id))
                            .ghost()
                            .icon(IconName::FolderOpen)
                            .tooltip("在 Finder 中显示")
                            .on_click(move |_, _, cx| cx.reveal_path(&path)),
                    )
                    .child(
                        Button::new(("remove", id))
                            .ghost()
                            .icon(IconName::Close)
                            .tooltip("移除")
                            .on_click(cx.listener(move |this, _, window, cx| this.confirm_remove(id, window, cx))),
                    )
                    .child(div().w_px().h_5().mx_2().bg(theme.border))
                    .child(
                        Switch::new(("switch", id))
                            .checked(running)
                            .on_change(cx.listener(move |this, on: &bool, window, cx| {
                                this.set_running(idx, *on, window, cx)
                            })),
                    ),
            )
    }
}

impl Render for HttpServerApp {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.sites.len();
        let running = self.sites.iter().filter(|s| s.running()).count();

        let body = if count == 0 {
            self.render_empty(cx).into_any_element()
        } else {
            let mut list = if self.grid {
                div().flex().flex_row().flex_wrap().gap_4().p_6()
            } else {
                v_flex().gap_3().p_6()
            };
            for idx in 0..count {
                list = if self.grid {
                    list.child(self.render_card(idx, cx))
                } else {
                    list.child(self.render_site(idx, cx))
                };
            }
            div()
                .id("sites")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(list)
                .into_any_element()
        };

        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                this.add_paths(paths.paths().to_vec(), window, cx);
            }))
            .child(
                TitleBar::new().child(
                    h_flex()
                        .w_full()
                        .pr_3()
                        .items_center()
                        .justify_between()
                        .child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(div().font_semibold().child("静态服务器"))
                                .when(count > 0, |this| {
                                    this.child(
                                        div()
                                            .text_sm()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(format!("{running}/{count} 运行中")),
                                    )
                                }),
                        )
                        .when(count > 0, |this| {
                            this.child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(
                                        ButtonGroup::new("view")
                                            .outline()
                                            .small()
                                            .child(Button::new("view-list").label("列表").selected(!self.grid))
                                            .child(Button::new("view-grid").label("网格").selected(self.grid))
                                            .on_click(cx.listener(|this, ix: &Vec<usize>, _, cx| {
                                                this.grid = ix.first() == Some(&1);
                                                store::save_grid(this.grid);
                                                cx.notify();
                                            })),
                                    )
                                    .child(
                                Button::new("add")
                                    .primary()
                                    .small()
                                    .icon(IconName::Plus)
                                    .label("添加文件夹")
                                    .on_click(cx.listener(|this, _, window, cx| this.pick_folders(window, cx))),
                            ))
                        }),
                ),
            )
            .child(body)
    }
}

fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            gpui_kit::init(cx);
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();

            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(860.), px(600.)),
                    cx,
                ))),
                window_min_size: Some(size(px(640.), px(420.))),
                ..TitleBar::window_options()
            };
            gpui_kit::open_window(options, cx, |window, cx| {
                window.set_window_title("静态服务器");
                cx.new(|cx| HttpServerApp::new(window, cx))
            })
            .expect("Failed to open window");
            cx.activate(true);
        });
}
