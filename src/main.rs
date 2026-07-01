// 1. 确保模块名称与你的文件名一致
mod controller;

use controller::{VideoControl};
use gpui::*;
use gpui_component::{button::*, *};
use gpui_component::label::Label;
use image::RgbaImage;

pub struct SyncToolApp {
    controller: VideoControl,
    current_image: Option<RgbaImage>,
}

impl Render for SyncToolApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 1. 非阻塞地拉取所有新帧，只保留最新的一帧
        // ✅ 注意：字段名从 rgba_data 改为了 data，与 controller.rs 保持一致
        if let Some(rx) = &mut self.controller.decoded_frame_rx {
            while let Ok(frame) = rx.try_recv() {
                if let Some(img) = RgbaImage::from_raw(frame.width, frame.height, frame.data) {
                    self.current_image = Some(img);
                }
            }
        }

        // 2. 构建视频画面元素
        let video_element = if self.current_image.is_some() {
            // ✅ 临时方案：先画一个带颜色的矩形来验证视频帧是否成功接收
            // 后续可以替换为真正的 GPU 纹理渲染
            div()
                .size_full()
                .child(
                    canvas(
                        |_bounds, _, _| {},
                        |bounds, _, window, _| {
                            // 绘制一个深灰色矩形作为视频画面占位
                            window.paint_quad(fill(
                                bounds,
                                gpui::rgb(0x333333),
                            ));
                        }
                    )
                        .size_full()
                )
        } else {
            // 没有画面时显示占位符
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
                // ✅ 创建控制器并传入监听地址
                let mut controller = VideoControl::new("0.0.0.0:8888");

                // ✅ 启动后台接收和解码任务
                if let Err(e) = controller.start() {
                    eprintln!("启动视频控制失败: {}", e);
                }

                let view = cx.new(|_| SyncToolApp {
                    controller,
                    current_image: None,
                });
                cx.new(|cx| Root::new(view, window, cx))
            })?;
            Ok::<_, anyhow::Error>(())
        })
            .detach();
    });
}