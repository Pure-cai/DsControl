use std::io::{Read, Write};
use std::net::UdpSocket;
use std::thread;
use std::sync::mpsc;
use std::any::Any;
use ffmpeg_sidecar::command::FfmpegCommand;

#[derive(Debug)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
    pub _pts: i64,
}

pub struct VideoReceiver {
    pub listen_addr: String,
    pub decoded_frame_rx: Option<mpsc::Receiver<VideoFrame>>,
    _ffmpeg_handle: Option<Box<dyn Any>>,
}

impl VideoReceiver {
    pub fn new(listen_addr: &str) -> Self {
        Self {
            listen_addr: listen_addr.to_string(),
            decoded_frame_rx: None,
            _ffmpeg_handle: None,
        }
    }

    pub fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("📡 开始监听视频流: {}", self.listen_addr);

        let (tx, rx) = mpsc::channel::<VideoFrame>();
        self.decoded_frame_rx = Some(rx);

        // 2. 启动 FFmpeg 解码子进程
        let mut ffmpeg_handle = FfmpegCommand::new()
            .arg("-f").arg("h264")
            .arg("-err_detect").arg("ignore_err")
            .arg("-i").arg("-")
            .arg("-f").arg("rawvideo")
            .arg("-pix_fmt").arg("rgba")
            .arg("-")
            .spawn()?;

        // ✅ 关键调整：在将句柄存入结构体（被 Any 包装）之前，先把管道拿走！
        let mut stdin = ffmpeg_handle.take_stdin().expect("Failed to get FFmpeg stdin");
        let mut stdout = ffmpeg_handle.take_stdout().expect("Failed to get FFmpeg stdout");
        let stderr_opt = ffmpeg_handle.take_stderr();

        // 现在把句柄存入结构体，防止它被 Drop
        self._ffmpeg_handle = Some(Box::new(ffmpeg_handle));

        // 3. 绑定 UDP Socket 接收数据
        let socket = UdpSocket::bind(&self.listen_addr)?;
        let mut buf = [0u8; 65535];
        println!("✅ UDP 监听已启动，等待发送端数据...");

        // 4. 后台任务：持续接收 UDP 数据并写入 FFmpeg stdin
        thread::spawn(move || {
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

        // 5. 后台任务：打印 FFmpeg 的 stderr
        if let Some(mut stderr) = stderr_opt {
            thread::spawn(move || {
                let mut err_buf = [0u8; 4096];
                loop {
                    match stderr.read(&mut err_buf) {
                        Ok(0) | Err(_) => break,
                        Ok(size) => {
                            if let Ok(msg) = std::str::from_utf8(&err_buf[..size]) {
                                print!("[FFmpeg Stderr] {}", msg);
                            }
                        }
                    }
                }
            });
        }

        // 6. 后台阻塞任务：直接从 stdout 读取 FFmpeg 输出的原始 RGBA 字节流
        thread::spawn(move || {
            let width = 1920;
            let height = 1080;
            let frame_size = (width * height * 4) as usize;
            let mut buffer = vec![0u8; frame_size];

            loop {
                match std::io::Read::read_exact(&mut stdout, &mut buffer) {
                    Ok(_) => {
                        let video_frame = VideoFrame {
                            width,
                            height,
                            data: std::mem::replace(&mut buffer, vec![0u8; frame_size]),
                            _pts: 0,
                        };
                        if tx.send(video_frame).is_err() {
                            eprintln!("❌ UI 接收端已关闭，停止发送帧");
                            break;
                        }
                    }
                    Err(e) => {
                        eprintln!("❌ 读取 FFmpeg 输出失败: {}", e);
                        break;
                    }
                }
            }
        });

        Ok(())
    }
}