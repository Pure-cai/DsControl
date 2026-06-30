// src/main.rs
mod controller;

use controller::{SyncController, VideoFrame};
use gpui::*;
use gpui_component::{button::*, label::Label, *};

pub struct SyncToolApp {
    controller: SyncController,
    current_frame: Option<VideoFrame>,
}

impl Render for SyncToolApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 1. 非阻塞地拉取所有新帧，只保留最新的一帧
        if let Some(rx) = &mut self.controller.decoded_frame_rx {
            // 使用 try_recv 避免阻塞 UI 线程
            // 如果积压了多帧，只取最新的一帧，丢弃中间的旧帧
            while let Ok(frame) = rx.try_recv() {
                // 将 frame.rgba_data 更新到你的 GPU 纹理中
                // 例如：self.texture.update(&frame.rgba_data);
                println!("收到解码帧: {}x{}", frame.width, frame.height);
            }
        }

        // 2. 构建视频画面元素 (统一使用 div 作为外层容器)
        let video_element = if let Some(_frame) = &self.current_frame {
            // 用 div 包裹 canvas，使其返回类型为 Div
            div()
                .size_full()
                .child(
                    canvas(
                        |_bounds, _, _| {},
                        |bounds, _, window, _| {
                            // 绘制一个带颜色的矩形来模拟视频画面
                            window.paint_quad(fill(
                                bounds,
                                gpui::rgb(0x333333), // 深灰色背景
                            ));
                        }
                    )
                        .size_full()
                )
        } else {
            // 用 div 包裹占位符，使其返回类型也为 Div
            div()
                .size_full()
                .v_flex()
                .items_center()
                .justify_center()
                .child(Label::new("等待视频流...").text_color(gpui::rgb(0x888888)))
        };

        // 3. 组装最终 UI
        div()
            .v_flex()
            .size_full()
            .gap_4()
            .p_4()
            .items_center()
            .child(
                Label::new("局域网同步控制工具")
                    .text_xl()
                    .font_weight(FontWeight::BOLD),
            )
            .child(
                Button::new("stream_btn")
                    .primary()
                    .label(if self.controller.is_streaming { "停止传输" } else { "开始传输" })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.controller.toggle_stream();
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .size_full()
                    .flex_grow()
                    .bg(gpui::rgb(0x111111))
                    .rounded_md()
                    .overflow_hidden()
                    .child(video_element),
            )
    }
}

fn main() {
    let app = Application::new();
    app.run(move |cx| {
        gpui_component::init(cx);
        cx.spawn(async move |cx| {
            cx.open_window(WindowOptions::default(), |window, cx| {
                let view = cx.new(|_| SyncToolApp {
                    controller: SyncController::new(),
                    current_frame: None,
                });
                cx.new(|cx| Root::new(view, window, cx))
            })?;
            Ok::<_, anyhow::Error>(())
        })
            .detach();
    });
}