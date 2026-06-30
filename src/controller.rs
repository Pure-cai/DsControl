// src/controller.rs
use tokio::sync::mpsc;
use tokio::net::UdpSocket;
use serde::{Deserialize, Serialize};
use std::thread;
use ffmpeg_next::{self as ffmpeg, codec, Packet, format};

/// 压缩后的视频帧数据结构（需与发送端保持一致）
#[derive(Clone, Serialize, Deserialize)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>, // H.264 编码后的数据
    pub pts: i64,
}

/// 解码后用于 UI 渲染的原始帧
#[derive(Clone)]
pub struct DecodedFrame {
    pub width: u32,
    pub height: u32,
    pub rgba_data: Vec<u8>,
}

pub struct SyncController {
    pub is_streaming: bool,
    // 用于向 UI 线程传递解码后 RGBA 帧的通道接收端
    pub decoded_frame_rx: Option<mpsc::Receiver<DecodedFrame>>,
}

impl SyncController {
    pub fn new() -> Self {
        Self {
            is_streaming: false,
            decoded_frame_rx: None,
        }
    }

    pub fn toggle_stream(&mut self) {
        self.is_streaming = !self.is_streaming;

        if self.is_streaming {
            println!("🟢 开始监听 UDP 视频流...");

            // 1. 创建 UI 通道：解码器 -> UI
            let (ui_tx, ui_rx) = mpsc::channel::<DecodedFrame>(5);
            self.decoded_frame_rx = Some(ui_rx);

            // 2. 创建网络通道：UDP 接收 -> 解码器
            let (net_tx, net_rx) = mpsc::channel::<VideoFrame>(30);

            // 3. 启动后台 Tokio 任务：负责 UDP 接收
            tokio::spawn(async move {
                let socket = match UdpSocket::bind("0.0.0.0:9000").await {
                    Ok(s) => s,
                    Err(e) => { eprintln!("❌ 绑定 UDP 端口失败: {}", e); return; }
                };
                println!("✅ 成功绑定 UDP 端口 9000，等待数据...");
                let mut buf = vec![0u8; 65535];

                loop {
                    match socket.recv_from(&mut buf).await {
                        Ok((len, _addr)) => {
                            match bincode::deserialize::<VideoFrame>(&buf[..len]) {
                                Ok(frame) => {
                                    if net_tx.send(frame).await.is_err() { break; }
                                }
                                Err(e) => eprintln!("⚠️ 解析数据失败: {}", e),
                            }
                        }
                        Err(e) => { eprintln!("❌ UDP 接收出错: {}", e); break; }
                    }
                }
            });

            // 4. 启动独立系统线程：负责 FFmpeg H.264 解码
            thread::spawn(move || {
                if let Err(e) = Self::decode_loop(net_rx, ui_tx) {
                    eprintln!("🔴 解码线程崩溃: {}", e);
                }
            });

        } else {
            println!("🛑 停止接收视频流。");
            self.decoded_frame_rx = None;
        }
    }

    /// FFmpeg 解码核心循环（运行在独立系统线程中）
    fn decode_loop(
        mut net_rx: mpsc::Receiver<VideoFrame>,
        ui_tx: mpsc::Sender<DecodedFrame>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        ffmpeg::init()?;

        // 查找 H.264 解码器并打开
        let codec = codec::decoder::find(codec::Id::H264).ok_or("找不到 H.264 解码器")?;
        let mut decoder = codec.video()?;

        // 创建 YUV420P 到 RGBA 的色彩空间转换器
        let mut scaler: Option<ffmpeg::software::scaling::Context> = None;

        // 阻塞等待网络通道传来的 H.264 数据包
        while let Some(frame) = net_rx.blocking_recv() {
            let mut packet = Packet::empty();
            // 将接收到的字节流包装成 FFmpeg Packet
            // 注意：这里假设 data 包含完整的 NAL 单元
            // 实际生产中可能需要处理 Annex B 格式的起始码
            unsafe {
                let data = frame.data.as_ptr();
                let size = frame.data.len();
                // 这里为了简化，直接利用 FFmpeg 的内部 API 填充 packet
                // 更安全的做法是使用 ffmpeg_next 提供的 API
            }

            // 使用 ffmpeg-next 的标准方式处理数据
            // 由于 Packet 的构造较复杂，这里我们采用简化的 send/receive 模式
            // 注意：实际工程中需要正确处理 Packet 的内存生命周期
            // 这里为了展示流程，假设我们已经成功将数据送入解码器

            // 【关键】由于 ffmpeg-next 的 Packet API 限制，
            // 实际使用时通常配合 format::context::Input 来读取。
            // 为了在纯内存中解码，我们需要手动构造 Packet：
            let mut pkt = Packet::new(frame.data.len());
            pkt.data_mut().copy_from_slice(&frame.data);
            pkt.set_pts(Some(frame.pts));

            decoder.send_packet(&pkt)?;

            let mut decoded_frame = ffmpeg::util::frame::Video::empty();
            while decoder.receive_frame(&mut decoded_frame).is_ok() {
                let w = decoded_frame.width();
                let h = decoded_frame.height();

                // 如果分辨率发生变化，重新创建 scaler
                if scaler.is_none() || scaler.as_ref().unwrap().width() != w || scaler.as_ref().unwrap().height() != h {
                    scaler = Some(ffmpeg::software::scaling::Context::get(
                        decoded_frame.format(), w, h,
                        format::Pixel::RGBA, w, h,
                        ffmpeg::software::scaling::Flags::BILINEAR,
                    )?);
                }

                // 转换为 RGBA
                let mut rgba_frame = ffmpeg::util::frame::Video::new(
                    format::Pixel::RGBA, w, h
                );
                scaler.as_mut().unwrap().run(&decoded_frame, &mut rgba_frame)?;

                // 提取 RGBA 原始字节
                let rgba_data = rgba_frame.data(0).to_vec();

                // 非阻塞地发送给 UI 线程
                let _ = ui_tx.blocking_send(DecodedFrame {
                    width: w,
                    height: h,
                    rgba_data,
                });
            }
        }
        Ok(())
    }
}