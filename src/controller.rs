use std::net::UdpSocket;
use std::io::Write;
use std::sync::mpsc;
use ffmpeg_sidecar::command::FfmpegCommand;

// 注意：这里的 VideoFrame 结构体必须与发送端完全一致
#[derive(Clone)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
    pub _pts: i64,
}

pub struct VideoControl {
    listen_addr: String,
    // ✅ 新增：用于记录当前是否正在流传输
    pub is_streaming: bool,
    pub decoded_frame_rx: Option<mpsc::Receiver<VideoFrame>>,
}

impl VideoControl {
    pub fn new(listen_addr: &str) -> Self {
        Self {
            listen_addr: listen_addr.to_string(),
            is_streaming: false, // 默认初始化为 false
            decoded_frame_rx: None,
        }
    }
    // ✅ 新增：切换流传输状态的方法
    pub fn toggle_stream(&mut self) {
        self.is_streaming = !self.is_streaming;

        if self.is_streaming {
            println!("▶️ 开始传输");
            // TODO: 在这里调用 self.start() 或恢复 FFmpeg 解码
            self.start().expect("TODO: panic message");
        } else {
            println!("⏹️ 停止传输");
            // TODO: 在这里暂停接收 UDP 数据或关闭 FFmpeg 子进程
        }
    }
    pub fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("📡 开始监听视频流: {}", self.listen_addr);

        // 1. 创建通道，用于将解码后的帧发送给 UI
        let (tx, rx) = mpsc::channel::<VideoFrame>();
        self.decoded_frame_rx = Some(rx);

        // 2. 启动 FFmpeg 解码子进程 (使用最基础的 .spawn() 写法)
        let mut ffmpeg_handle = FfmpegCommand::new()
            .arg("-f").arg("h264")
            .arg("-i").arg("-")
            .arg("-f").arg("rawvideo")
            .arg("-pix_fmt").arg("rgba")
            .arg("-")
            .spawn()?; // ✅ 使用最基础的 spawn

        let mut stdin = ffmpeg_handle.take_stdin().expect("Failed to get FFmpeg stdin");


        // 3. 绑定 UDP Socket 接收数据
        let socket = UdpSocket::bind(&self.listen_addr)?;
        let mut buf = [0u8; 65535];

        // 4. 后台任务：持续接收 UDP 数据并写入 FFmpeg 管道
        std::thread::spawn(move || {
            loop {
                match socket.recv_from(&mut buf) {
                    Ok((size, _)) => {
                        if stdin.write_all(&buf[..size]).is_err() {
                            eprintln!("❌ 写入 FFmpeg 管道失败，可能子进程已退出");
                            break;
                        }
                    }
                    Err(e) => eprintln!("❌ UDP 接收失败: {}", e),
                }
            }
        });

        // 5. 后台阻塞任务：直接从 stdout 读取 FFmpeg 输出的原始 RGBA 字节流
        std::thread::spawn(move || {
            // 获取 FFmpeg 的标准输出管道
            let mut stdout = ffmpeg_handle.take_stdout().expect("Failed to get FFmpeg stdout");

            // ⚠️ 注意：你需要知道 FFmpeg 输出的视频分辨率。
            // 这里假设是 1920x1080，实际应用中你需要通过其他方式（如 SPS/PPS 解析或配置）获取
            let width = 1920;
            let height = 1080;
            // RGBA 格式，每个像素 4 个字节
            let frame_size = (width * height * 4) as usize;
            let mut buffer = vec![0u8; frame_size];

            loop {
                // 精确读取一帧的数据
                match std::io::Read::read_exact(&mut stdout, &mut buffer) {
                    Ok(_) => {
                        let video_frame = VideoFrame {
                            width,
                            height,
                            data: buffer.clone(), // 将这一帧的数据发送出去
                            _pts: 0,
                        };
                        // 发送帧，如果 UI 处理不过来就忽略错误
                        let _ = tx.send(video_frame);
                    }
                    Err(e) => {
                        eprintln!("❌ 读取 FFmpeg 输出失败: {}", e);
                        break; // 读取失败说明 FFmpeg 进程可能退出了
                    }
                }
            }
        });

        Ok(())
    }
}