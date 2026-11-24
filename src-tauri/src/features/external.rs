//! 外部语音服务：拉起用户本机的 `sherpa-onnx-offline-websocket-server` 进程，
//! 经本机回环 `ws` 转写（GPU 加速走这里）。
//!
//! 具体业务功能，故放在 `features`；只能依赖 `cores`（配置、受管子进程）。
//! 进程句柄由 `cores::supervised_child` 单槽位托管，停止服务与退出进程两路都会查杀。
//! 与服务端约定的线协议见官方离线 websocket 文档（首帧 8 字节头 + 后续音频帧）：
//! 前 4 字节小端为采样率，后 4 字节小端为音频总字节数，之后为 `f32` 小端采样；
//! 服务端回一个文本帧（空结果回单个空格），客户端以 `"Done"` 结束本连接。

use anyhow::Context;
use futures_util::{SinkExt, StreamExt};

/// 外部服务就绪轮询间隔毫秒
const HEALTH_POLL_MS: u64 = 200;
/// 外部服务启动超时秒：大模型冷加载兜底，超时查杀并报错
const STARTUP_TIMEOUT_SECS: u64 = 300;
/// 服务日志文件名：落应用数据目录，避免在进程工作目录写 `./log.txt`
pub const SERVER_LOG_FILE_NAME: &str = "external-server.log";
/// 单个 `ws` 音频帧上限字节：超长音频分片发送，服务端按首帧声明的总字节数拼包
const WS_CHUNK_BYTES: usize = 32 * 1024;
/// `FunASR Nano` 采样参数：与内嵌识别器配置（`asr::build_recognizer_config`）同值，
///
/// 两边走同一组超参，内外模式输出口径一致
const FUNASR_MAX_NEW_TOKENS: u32 = 512;
/// `FunASR Nano` 采样温度：与内嵌配置同值
const FUNASR_TEMPERATURE: &str = "1e-06";
/// `FunASR Nano` 采样 `top-p`：与内嵌配置同值
const FUNASR_TOP_P: &str = "0.8";
/// `FunASR Nano` 随机种子：与内嵌配置同值
const FUNASR_SEED: u32 = 42;
/// `Qwen3-ASR` 生成长度：短音频不及时吐 EOS 会拖出上千 token
/// （1.7B 在 4070 上约 7 秒），256 覆盖约 30 秒中文语音（远超语音指令与测试录制
/// 的典型长度），把最坏情况收敛到约 2 秒；超长音频会被截断（已知取舍）。
/// 服务端默认 128 更激进，内嵌与外部保持一致。
const QWEN3_MAX_NEW_TOKENS: u32 = 256;
/// `Qwen3-ASR` 总长度上限：与内嵌配置同值（服务端默认 512 不够用）
const QWEN3_MAX_TOTAL_LEN: u32 = 8192;
/// `Qwen3-ASR` 采样温度：显式传入，免受服务端默认值漂移影响
const QWEN3_TEMPERATURE: &str = "1e-06";
/// `Qwen3-ASR` 采样 `top-p`：显式传入，免受服务端默认值漂移影响
const QWEN3_TOP_P: &str = "0.8";
/// `Qwen3-ASR` 随机种子：显式传入，免受服务端默认值漂移影响
const QWEN3_SEED: u32 = 42;

/// 外部解码结果：文本 + 音频时长 + 推理耗时；调用方（`asr`）再套后处理链
pub struct ExternalDecode {
    /// 识别文本（句末空格已清，空音频为空串，不算失败）
    pub text: String,
    /// 音频时长秒（展示用）
    pub audio_sec: f64,
    /// 推理耗时毫秒（含本机回环往返）
    pub infer_ms: u32,
}

/// 解析生效线程数：配置 `0` 表示自动（跟随内部模式的核数减一），其余原样生效
pub fn resolve_threads(configured: u32) -> i32 {
    if configured == 0 {
        return crate::features::asr::num_cpus();
    }
    i32::try_from(configured).unwrap_or(1)
}

/// 组装服务端启动参数：文件路径全部派生自模型目录，不手写绝对路径；
///
/// `--model-type` 故意不传：三族都不在服务端的可选值里，传未知值反而导致模型加载两次。
/// 语言映射复用内嵌识别器的同一口径（`sense-voice` 用 `zh`/`yue`，`FunASR` 用中文提示词）。
/// `Qwen3-ASR` 的语言只能按内嵌 `stream` 设置，服务端无对应启动参数，走服务端默认。
pub fn build_server_args(
    spec: &'static crate::features::models::ModelSpec,
    dir: &std::path::Path,
    language: crate::cores::config::AsrLanguage,
    threads: i32,
    provider: crate::cores::config::ServiceProvider,
    port: u16,
    log_file: &std::path::Path,
) -> Vec<String> {
    use crate::features::models::ModelCategory;

    let file = |name: &str| dir.join(name).to_string_lossy().into_owned();
    let mut args = vec![
        format!("--port={port}"),
        format!("--num-work-threads={threads}"),
        format!("--provider={}", provider.as_str()),
        "--decoding-method=greedy_search".to_owned(),
        format!("--log-file={}", log_file.to_string_lossy()),
    ];
    match spec.category {
        ModelCategory::FunasrNano => {
            push_funasr_args(&mut args, spec, &file, language);
        }
        ModelCategory::Qwen3Asr => {
            push_qwen3_args(&mut args, &file);
        }
        ModelCategory::SenseVoice => {
            push_sense_voice_args(&mut args, spec, &file, language);
        }
    }
    args
}

/// `FunASR Nano` 族参数：编码器适配器 / 嵌入 / 大模型 / 分词器目录 + 与内嵌一致的采样参数
fn push_funasr_args(
    args: &mut Vec<String>,
    spec: &'static crate::features::models::ModelSpec,
    file: &dyn Fn(&str) -> String,
    language: crate::cores::config::AsrLanguage,
) {
    args.push(format!(
        "--funasr-nano-encoder-adaptor={}",
        file(spec.funasr_encoder_adaptor)
    ));
    args.push(format!(
        "--funasr-nano-embedding={}",
        file(spec.funasr_embedding)
    ));
    args.push(format!("--funasr-nano-llm={}", file(spec.funasr_llm)));
    args.push(format!("--funasr-nano-tokenizer={}", file("Qwen3-0.6B")));
    args.push(format!(
        "--funasr-nano-max-new-tokens={FUNASR_MAX_NEW_TOKENS}"
    ));
    args.push(format!("--funasr-nano-temperature={FUNASR_TEMPERATURE}"));
    args.push(format!("--funasr-nano-top-p={FUNASR_TOP_P}"));
    args.push(format!("--funasr-nano-seed={FUNASR_SEED}"));
    args.push("--funasr-nano-itn=true".to_owned());
    if let Some(prompt_language) = crate::features::asr::funasr_nano_language(language) {
        args.push(format!("--funasr-nano-language={prompt_language}"));
    }
}

/// `Qwen3-ASR` 族参数：卷积前端 / 编码器 / 解码器 / 分词器目录 + 放大的生成长度
fn push_qwen3_args(args: &mut Vec<String>, file: &dyn Fn(&str) -> String) {
    args.push(format!(
        "--qwen3-asr-conv-frontend={}",
        file("conv_frontend.onnx")
    ));
    args.push(format!("--qwen3-asr-encoder={}", file("encoder.int8.onnx")));
    args.push(format!("--qwen3-asr-decoder={}", file("decoder.int8.onnx")));
    args.push(format!("--qwen3-asr-tokenizer={}", file("tokenizer")));
    args.push(format!("--qwen3-asr-max-new-tokens={QWEN3_MAX_NEW_TOKENS}"));
    args.push(format!("--qwen3-asr-max-total-len={QWEN3_MAX_TOTAL_LEN}"));
    args.push(format!("--qwen3-asr-temperature={QWEN3_TEMPERATURE}"));
    args.push(format!("--qwen3-asr-top-p={QWEN3_TOP_P}"));
    args.push(format!("--qwen3-asr-seed={QWEN3_SEED}"));
}

/// `SenseVoice` 族参数：模型文件 / tokens / 语言 / 逆文本正则化开关
fn push_sense_voice_args(
    args: &mut Vec<String>,
    spec: &'static crate::features::models::ModelSpec,
    file: &dyn Fn(&str) -> String,
    language: crate::cores::config::AsrLanguage,
) {
    args.push(format!("--sense-voice-model={}", file(spec.model_file)));
    args.push(format!(
        "--sense-voice-language={}",
        crate::features::asr::sense_voice_language(spec, language)
    ));
    args.push(format!("--sense-voice-use-itn={}", spec.use_itn));
    args.push(format!("--tokens={}", file("tokens.txt")));
}

/// 校验服务端程序路径：空串视为未配置，不存在或非文件直接拒绝（`spawn` 的报错更晦涩）
pub fn validate_program(program: &str) -> anyhow::Result<()> {
    if program.trim().is_empty() {
        anyhow::bail!("external server path is not configured");
    }
    if !std::path::Path::new(program).is_file() {
        anyhow::bail!("external server program not found: {program}");
    }
    Ok(())
}

/// 端口预检：已被占用直接拒绝，避免误杀用户手动起的服务；
///
/// 预检与服务端实际绑定之间存在竞态，兜底靠就绪轮询超时（服务端绑不上端口就永远不就绪）
fn ensure_port_free(port: u16) -> anyhow::Result<()> {
    match std::net::TcpListener::bind(("127.0.0.1", port)) {
        Ok(_) => Ok(()),
        Err(_) => anyhow::bail!(
            "port {port} is already in use, pick another one or stop the existing service"
        ),
    }
}

/// 拉起服务端进程并等到就绪：日志落应用数据目录；就绪超时查杀并报错；
///
/// 进程句柄进 `supervised_child` 单槽位托管，停止服务与退出进程两路都会查杀
pub async fn spawn_and_wait_ready(
    program: &str,
    args: &[String],
    port: u16,
    log_path: &std::path::Path,
) -> anyhow::Result<()> {
    ensure_port_free(port)?;
    let log = std::fs::File::create(log_path).with_context(|| {
        format!(
            "failed to create external server log: {}",
            log_path.display()
        )
    })?;
    let child = std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(log.try_clone().context("failed to dup server log handle")?)
        .stderr(log)
        .spawn()
        .context("failed to launch external server (check program path and execute permission)")?;
    crate::cores::supervised_child::set_child(child);
    if let Err(err) = wait_until_ready(port).await {
        crate::cores::supervised_child::kill_child();
        return Err(err);
    }
    Ok(())
}

/// 就绪轮询：直连 `ws` 握手，通即就绪；超时按启动失败结算（调用方负责查杀）
async fn wait_until_ready(port: u16) -> anyhow::Result<()> {
    let url = format!("ws://127.0.0.1:{port}");
    let deadline = std::time::Instant::now()
        .checked_add(std::time::Duration::from_secs(STARTUP_TIMEOUT_SECS))
        .unwrap_or_else(std::time::Instant::now);
    loop {
        if let Ok((mut ws, _)) = tokio_tungstenite::connect_async(&url).await {
            let _ = ws.close(None).await;
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            anyhow::bail!(
                "external server did not become ready within {STARTUP_TIMEOUT_SECS}s (port {port})"
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(HEALTH_POLL_MS)).await;
    }
}

/// 经本机回环转写单个音频文件：读 `wav` → 按线协议分片发送 → 取回文本；
///
/// 采样率由服务端自行适配（与内嵌识别器同口径，无需客户端重采样）；
/// 空结果按空字符串返回，不算失败
pub async fn decode_file(port: u16, path: &std::path::Path) -> anyhow::Result<ExternalDecode> {
    // 音频先转自有数据再跨 `await`：`Wave` 底层是裸指针、非 `Send`，不能进 `spawn` 的任务
    let (sample_rate, samples) = read_wav_samples(path)?;
    let payload: Vec<u8> = samples
        .iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect();
    let total = i32::try_from(payload.len()).context("audio file too large for server")?;
    let started = std::time::Instant::now();
    let text = send_and_receive(port, sample_rate, &payload, total).await?;
    let infer_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    // 音频时长只用于展示，超长音频的尾数精度可接受
    #[allow(clippy::cast_precision_loss)]
    let audio_sec = samples.len() as f64 / f64::from(sample_rate);
    Ok(ExternalDecode {
        text,
        audio_sec: (audio_sec * 100.0).round() / 100.0,
        infer_ms: u32::try_from(infer_ms).unwrap_or(u32::MAX),
    })
}

/// 读 `wav` 为自有采样：采样率 + `f32` 单声道采样（服务端按采样率自行适配）
fn read_wav_samples(path: &std::path::Path) -> anyhow::Result<(i32, Vec<f32>)> {
    let wave =
        sherpa_onnx::Wave::read(&path.to_string_lossy()).context("failed to read wav file")?;
    Ok((wave.sample_rate(), wave.samples().to_vec()))
}

/// 从服务端回执提文本：C++ 服务端回 JSON 包（`text`/`timestamps`/`tokens`…），
/// 空结果的 `text` 为空串；非 JSON 回执按原文返回，单空格按空处理
fn extract_text(reply: &str) -> String {
    let trimmed = reply.trim();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed)
        && let Some(text) = value.get("text").and_then(serde_json::Value::as_str)
    {
        return text.trim().to_owned();
    }
    trimmed.to_owned()
}

/// 一次问答：建连 → 发首帧（采样率 + 总字节数 + 首片音频）→ 分片发剩余 → 收文本 → 发 `Done` 关连
async fn send_and_receive(
    port: u16,
    sample_rate: i32,
    payload: &[u8],
    total: i32,
) -> anyhow::Result<String> {
    use tokio_tungstenite::tungstenite::Message;

    let url = format!("ws://127.0.0.1:{port}");
    let (mut ws, _) = tokio_tungstenite::connect_async(&url)
        .await
        .context("failed to connect to external server")?;
    let mut first = Vec::with_capacity(8 + payload.len().min(WS_CHUNK_BYTES));
    first.extend_from_slice(&sample_rate.to_le_bytes());
    first.extend_from_slice(&total.to_le_bytes());
    let (head, tail) = payload.split_at(payload.len().min(WS_CHUNK_BYTES));
    first.extend_from_slice(head);
    ws.send(Message::Binary(first.into()))
        .await
        .context("failed to send audio header")?;
    for chunk in tail.chunks(WS_CHUNK_BYTES) {
        ws.send(Message::Binary(chunk.to_vec().into()))
            .await
            .context("failed to send audio chunk")?;
    }
    let text = loop {
        match ws
            .next()
            .await
            .context("server closed connection without result")??
        {
            Message::Text(reply) => break extract_text(&reply),
            Message::Close(_) => anyhow::bail!("server closed connection without result"),
            _ => {}
        }
    };
    let _ = ws.send(Message::Text("Done".into())).await;
    let _ = ws.close(None).await;
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::models::ModelCategory;

    /// 取清单里现成的规格，避免手造 `'static` 引用
    fn spec_of(id: &str) -> &'static crate::features::models::ModelSpec {
        crate::features::models::find_spec(id).expect("registry spec exists")
    }

    #[test]
    fn threads_zero_follows_cpu_count() {
        assert!(resolve_threads(0) > 0);
        assert_eq!(resolve_threads(4), 4);
    }

    #[test]
    fn funasr_args_cover_model_files() {
        let dir = std::path::Path::new("/models/funasr");
        let args = build_server_args(
            spec_of("funasr-nano-int8"),
            dir,
            crate::cores::config::AsrLanguage::Chinese,
            3,
            crate::cores::config::ServiceProvider::Cpu,
            6006,
            std::path::Path::new("/logs/server.log"),
        );
        let has = |flag: &str| args.iter().any(|arg| arg == flag);
        assert!(has("--port=6006"));
        assert!(has("--num-work-threads=3"));
        assert!(has("--provider=cpu"));
        assert!(has("--decoding-method=greedy_search"));
        assert!(has("--log-file=/logs/server.log"));
        assert!(has(
            "--funasr-nano-encoder-adaptor=/models/funasr/encoder_adaptor.int8.onnx"
        ));
        assert!(has(
            "--funasr-nano-embedding=/models/funasr/embedding.int8.onnx"
        ));
        assert!(has("--funasr-nano-llm=/models/funasr/llm.int8.onnx"));
        assert!(has("--funasr-nano-tokenizer=/models/funasr/Qwen3-0.6B"));
        assert!(has("--funasr-nano-language=中文"));
        assert!(!args.iter().any(|arg| arg.starts_with("--model-type")));
    }

    #[test]
    fn funasr_auto_language_omits_prompt_flag() {
        let args = build_server_args(
            spec_of("funasr-nano-int8"),
            std::path::Path::new("/models/funasr"),
            crate::cores::config::AsrLanguage::Auto,
            3,
            crate::cores::config::ServiceProvider::Cpu,
            6006,
            std::path::Path::new("/logs/server.log"),
        );
        assert!(
            !args
                .iter()
                .any(|arg| arg.starts_with("--funasr-nano-language"))
        );
    }

    #[test]
    fn qwen3_args_bound_generation_length() {
        let args = build_server_args(
            spec_of("qwen3-1.7b-int8"),
            std::path::Path::new("/models/qwen3"),
            crate::cores::config::AsrLanguage::Chinese,
            2,
            crate::cores::config::ServiceProvider::Cuda,
            7006,
            std::path::Path::new("/logs/server.log"),
        );
        let has = |flag: &str| args.iter().any(|arg| arg == flag);
        assert!(has("--provider=cuda"));
        assert!(has("--qwen3-asr-encoder=/models/qwen3/encoder.int8.onnx"));
        assert!(has("--qwen3-asr-decoder=/models/qwen3/decoder.int8.onnx"));
        assert!(has(
            "--qwen3-asr-conv-frontend=/models/qwen3/conv_frontend.onnx"
        ));
        assert!(has("--qwen3-asr-tokenizer=/models/qwen3/tokenizer"));
        assert!(has("--qwen3-asr-max-new-tokens=256"));
        assert!(has("--qwen3-asr-max-total-len=8192"));
    }

    #[test]
    fn sense_voice_args_follow_spec_and_language() {
        let cantonese = build_server_args(
            spec_of("sense-voice-yue-int8"),
            std::path::Path::new("/models/sense"),
            crate::cores::config::AsrLanguage::Chinese,
            3,
            crate::cores::config::ServiceProvider::Cpu,
            6006,
            std::path::Path::new("/logs/server.log"),
        );
        let has = |flag: &str| cantonese.iter().any(|arg| arg == flag);
        assert!(has("--sense-voice-model=/models/sense/model.int8.onnx"));
        assert!(has("--sense-voice-language=yue"));
        assert!(has("--tokens=/models/sense/tokens.txt"));

        let auto = build_server_args(
            spec_of("sense-voice-int8"),
            std::path::Path::new("/models/sense"),
            crate::cores::config::AsrLanguage::Auto,
            3,
            crate::cores::config::ServiceProvider::Cpu,
            6006,
            std::path::Path::new("/logs/server.log"),
        );
        assert!(auto.iter().any(|arg| arg == "--sense-voice-language=auto"));
        assert!(auto.iter().any(|arg| arg == "--sense-voice-use-itn=true"));
    }

    #[test]
    fn extract_text_handles_json_envelope_and_plain_replies() {
        assert_eq!(
            extract_text(r#"{"text": "你好世界", "tokens": ["你"]}"#),
            "你好世界"
        );
        assert_eq!(extract_text(r#"{"text": "  开饭  "}"#), "开饭");
        assert_eq!(extract_text(r#"{"text": ""}"#), "");
        assert_eq!(extract_text(" "), "");
        assert_eq!(extract_text("plain text"), "plain text");
        assert_eq!(extract_text(r#"{"no_text": 1}"#), r#"{"no_text": 1}"#);
    }

    #[test]
    fn program_validation_rejects_blank_and_missing() {
        assert!(validate_program("").is_err());
        assert!(validate_program("   ").is_err());
        assert!(validate_program("/definitely/not/a/server/binary").is_err());
    }

    #[test]
    fn occupied_port_fails_precheck() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("ephemeral port binds");
        let port = listener.local_addr().expect("listener has addr").port();
        assert!(ensure_port_free(port).is_err());
    }

    #[test]
    fn category_coverage_is_total() {
        // 新增模型族必须同步补参数映射，缺 arm 直接编译失败
        for category in [
            ModelCategory::FunasrNano,
            ModelCategory::Qwen3Asr,
            ModelCategory::SenseVoice,
        ] {
            let id = match category {
                ModelCategory::FunasrNano => "funasr-nano-int8",
                ModelCategory::Qwen3Asr => "qwen3-1.7b-int8",
                ModelCategory::SenseVoice => "sense-voice-int8",
            };
            let args = build_server_args(
                spec_of(id),
                std::path::Path::new("/models/x"),
                crate::cores::config::AsrLanguage::Chinese,
                1,
                crate::cores::config::ServiceProvider::Coreml,
                6006,
                std::path::Path::new("/logs/server.log"),
            );
            assert!(args.iter().any(|arg| arg == "--provider=coreml"));
        }
    }
}
