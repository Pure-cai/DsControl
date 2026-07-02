use gpui::*;
use std::sync::mpsc;

// 引入你写好的 VideoReceiver 和 VideoFrame
mod controller;
use controller::{VideoReceiver, VideoFrame};

// 定义你的主应用状态结构体
struct VideoPlayerApp {
    frame_rx: Option<mpsc::Receiver<VideoFrame>>,
    current_frame: Option<VideoFrame>, // 缓存当前最新的视频帧
}

impl VideoPlayerApp {
    // 初始化方法
    fn new(frame_rx: mpsc::Receiver<VideoFrame>) -> Self {
        Self {
            frame_rx: Some(frame_rx),
            current_frame: None,
        }
    }

    // 处理视频帧更新的方法（由定时器触发）
    fn poll_video_frame(&mut self, cx: &mut Context<Self>) {
        if let Some(rx) = &self.frame_rx {
            // 非阻塞地尝试获取最新的一帧（try_recv 避免阻塞 UI 线程）
            // 使用循环获取最新帧，丢弃中间积压的旧帧，保证低延迟
            while let Ok(frame) = rx.try_recv() {
                self.current_frame = Some(frame);
            }
        }
        // 通知 GPUI 状态已改变，触发重新渲染
        cx.notify();
    }
}

// 实现 GPUI 的 Render trait，定义 UI 的渲染逻辑
impl Render for VideoPlayerApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 每次渲染前，尝试拉取最新的视频帧
        self.poll_video_frame(cx);

        // 使用 gpui-component 的 div 构建布局
        div()
            .size_full()
            .bg(rgb(0x111111)) // 设置深色背景
            .flex()
            .items_center()
            .justify_center()
            .child(
                if let Some(_frame) = &self.current_frame {
                    // 💡 核心渲染逻辑：
                    // 在实际开发中，你需要将 frame.data (RGBA 字节)
                    // 上传到 GPU 纹理，并使用 gpui 的 img() 或自定义 Canvas 元素进行渲染。
                    // 这里先使用文本作为占位符，验证数据链路是否打通。
                    div()
                        .text_color(gpui::white())
                        .child(format!(
                            "🎬 视频流已连接！\n分辨率: {}x{}\n数据大小: {} bytes",
                            _frame.width,
                            _frame.height,
                            _frame.data.len()
                        ))
                } else {
                    div()
                        .text_color(gpui::white())
                        .child("📡 正在等待视频流...")
                }
            )
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 初始化视频接收端
    let mut receiver = VideoReceiver::new("0.0.0.0:8888");
    receiver.start()?;
    let frame_rx = receiver.decoded_frame_rx.take().expect("未找到帧接收通道");

    println!("🖥️ GPUI 播放器已就绪...");

    // 2. 启动 GPUI 应用
    Application::new().run(|cx: &mut App| {
        // 创建应用状态
        let app_state = VideoPlayerApp::new(frame_rx);

        // 打开主窗口
        cx.open_window(
            WindowOptions {
                //title: Some("Rust GPUI 实时视频播放器".into()),
                focus: true,
                ..Default::default()
            },
            |_, cx| {
                // 将状态注册到 GPUI 的上下文中
                cx.new(|_| app_state)
            },
        )
            .unwrap();

        // 激活窗口
        cx.activate(true);
    });

    Ok(())
}