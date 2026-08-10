use std::time::Instant;
use std::{num::NonZeroU32, path::PathBuf, sync::mpsc, thread};

use anyhow::{Context, anyhow, bail};
use llama_cpp_2::{
    TokenToStringError,
    context::params::LlamaContextParams,
    llama_backend::LlamaBackend,
    llama_batch::LlamaBatch,
    model::{AddBos, LlamaChatMessage, LlamaChatTemplate, LlamaModel, params::LlamaModelParams},
    sampling::LlamaSampler,
    token::LlamaToken,
};
use opentelemetry::KeyValue;

const INITIAL_TOKEN_BUFFER_SIZE: usize = 32;

#[derive(Debug, Clone)]
pub struct LlamaRuntimeConfig {
    pub model_path: PathBuf,
    pub context_tokens: u32,
    pub threads: i32,
    pub n_gpu_layers: u32,
    pub chat_template_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LlamaRuntimeMessage {
    pub role: String,
    pub content: String,
}

#[derive(Clone)]
pub struct LlamaRuntime {
    request_tx: mpsc::Sender<LlamaRequest>,
}

struct LlamaRequest {
    messages: Vec<LlamaRuntimeMessage>,
    max_tokens: usize,
    response_tx: mpsc::Sender<anyhow::Result<String>>,
    // Span context does not cross the mpsc into the worker thread by itself;
    // the caller's span rides along so the generation span parents correctly.
    span: tracing::Span,
    enqueued_at: Instant,
}

struct LlamaWorker {
    _backend: LlamaBackend,
    model: LlamaModel,
    chat_template: LlamaChatTemplate,
    context_tokens: u32,
    threads: i32,
    /// The model file stem, as the `gen_ai.request.model` attribute. Never
    /// any message content.
    model_label: String,
}

impl LlamaRuntime {
    pub fn new(config: LlamaRuntimeConfig) -> anyhow::Result<Self> {
        let (request_tx, request_rx) = mpsc::channel();
        let (startup_tx, startup_rx) = mpsc::channel();

        thread::Builder::new()
            .name("wiab-llama".to_owned())
            .spawn(move || {
                let startup_result = LlamaWorker::new(config);
                match startup_result {
                    Ok(worker) => {
                        let _ = startup_tx.send(Ok(()));
                        worker.run(request_rx);
                    }
                    Err(err) => {
                        let _ = startup_tx.send(Err(err));
                    }
                }
            })
            .context("failed to spawn llama runtime thread")?;

        startup_rx
            .recv()
            .context("llama runtime thread exited before initialization completed")??;

        Ok(Self { request_tx })
    }

    pub fn generate(
        &self,
        messages: Vec<LlamaRuntimeMessage>,
        max_tokens: usize,
    ) -> anyhow::Result<String> {
        if messages.is_empty() {
            bail!("llama generation requires at least one chat message");
        }
        if max_tokens == 0 {
            bail!("llama generation requires max_tokens > 0");
        }

        let (response_tx, response_rx) = mpsc::channel();
        self.request_tx
            .send(LlamaRequest {
                messages,
                max_tokens,
                response_tx,
                span: tracing::Span::current(),
                enqueued_at: Instant::now(),
            })
            .map_err(|_| anyhow!("llama runtime is no longer accepting requests"))?;

        response_rx
            .recv()
            .context("llama runtime dropped the response channel before replying")?
    }
}

impl LlamaWorker {
    fn new(config: LlamaRuntimeConfig) -> anyhow::Result<Self> {
        let load_started = Instant::now();
        let mut backend = LlamaBackend::init()
            .map_err(|err| anyhow!("failed to initialize llama backend: {err}"))?;

        let model_params = LlamaModelParams::default().with_n_gpu_layers(config.n_gpu_layers);
        let model = LlamaModel::load_from_file(&backend, &config.model_path, &model_params)
            .with_context(|| {
                format!(
                    "failed to load llama model from '{}'",
                    config.model_path.display()
                )
            })?;
        let chat_template = model
            .chat_template(config.chat_template_name.as_deref())
            .map_err(|err| anyhow!("failed to load model chat template: {err}"))?;
        backend.void_logs();

        let model_label = config
            .model_path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| "unknown".to_owned());
        // One-shot, so a log line rather than a metric.
        tracing::info!(
            duration_ms = load_started.elapsed().as_millis() as u64,
            model = %model_label,
            "llama model loaded"
        );

        Ok(Self {
            _backend: backend,
            model,
            chat_template,
            context_tokens: config.context_tokens,
            threads: config.threads,
            model_label,
        })
    }

    fn run(self, request_rx: mpsc::Receiver<LlamaRequest>) {
        while let Ok(request) = request_rx.recv() {
            wiab_telemetry::metrics()
                .llama_queue_duration
                .record(request.enqueued_at.elapsed().as_secs_f64(), &[]);
            let span = tracing::info_span!(
                parent: &request.span,
                "chat",
                otel.name = %format_args!("chat {}", self.model_label),
                otel.status_code = tracing::field::Empty,
                gen_ai.operation.name = "chat",
                gen_ai.request.model = %self.model_label,
                gen_ai.request.max_tokens = request.max_tokens as u64,
                gen_ai.usage.input_tokens = tracing::field::Empty,
                gen_ai.usage.output_tokens = tracing::field::Empty,
            );
            let result =
                span.in_scope(|| self.generate(request.messages, request.max_tokens, &span));
            if result.is_err() {
                span.record("otel.status_code", "ERROR");
            }
            let _ = request.response_tx.send(result);
        }
    }

    fn generate(
        &self,
        messages: Vec<LlamaRuntimeMessage>,
        max_tokens: usize,
        span: &tracing::Span,
    ) -> anyhow::Result<String> {
        let started = Instant::now();
        let chat_messages = messages
            .into_iter()
            .map(|message| {
                LlamaChatMessage::new(message.role, message.content)
                    .map_err(|err| anyhow!("failed to build llama chat message: {err}"))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        let prompt = self
            .model
            .apply_chat_template(&self.chat_template, &chat_messages, true)
            .map_err(|err| anyhow!("failed to apply llama chat template: {err}"))?;
        let prompt_tokens = self
            .model
            .str_to_token(&prompt, AddBos::Never)
            .map_err(|err| anyhow!("failed to tokenize llama prompt: {err}"))?;
        if prompt_tokens.is_empty() {
            bail!("llama prompt tokenization produced zero tokens");
        }
        if prompt_tokens.len() >= self.context_tokens as usize {
            bail!(
                "llama prompt uses {} tokens but context only allows {}",
                prompt_tokens.len(),
                self.context_tokens
            );
        }

        span.record("gen_ai.usage.input_tokens", prompt_tokens.len() as u64);
        let batch_tokens = u32::try_from(prompt_tokens.len())
            .map_err(|_| anyhow!("prompt token count does not fit into u32"))?;
        let context_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(self.context_tokens))
            .with_n_batch(batch_tokens)
            .with_n_ubatch(batch_tokens)
            .with_n_threads(self.threads)
            .with_n_threads_batch(self.threads);
        let mut context = self
            .model
            .new_context(&self._backend, context_params)
            .map_err(|err| anyhow!("failed to create llama context: {err}"))?;

        let mut prompt_batch = LlamaBatch::get_one(&prompt_tokens)
            .map_err(|err| anyhow!("failed to create llama prompt batch: {err}"))?;
        context
            .decode(&mut prompt_batch)
            .map_err(|err| anyhow!("failed to decode llama prompt batch: {err}"))?;

        let model_attr = KeyValue::new("gen_ai.request.model", self.model_label.clone());
        // Prefill done: the next sample produces the first output token.
        wiab_telemetry::metrics()
            .genai_time_to_first_token
            .record(started.elapsed().as_secs_f64(), &[model_attr.clone()]);
        let decode_started = Instant::now();

        let mut sampler = LlamaSampler::greedy();
        let mut output_bytes = Vec::new();
        let mut output_tokens: u64 = 0;

        for _ in 0..max_tokens {
            let token = sampler.sample(&context, -1);
            if self.model.is_eog_token(token) {
                break;
            }
            sampler.accept(token);
            output_tokens += 1;

            output_bytes.extend(token_bytes(&self.model, token)?);

            let next_tokens = [token];
            let mut batch = LlamaBatch::get_one(&next_tokens)
                .map_err(|err| anyhow!("failed to create llama generation batch: {err}"))?;
            context
                .decode(&mut batch)
                .map_err(|err| anyhow!("failed to decode llama generation batch: {err}"))?;
        }

        span.record("gen_ai.usage.output_tokens", output_tokens);
        let telemetry = wiab_telemetry::metrics();
        telemetry.genai_token_usage.record(
            prompt_tokens.len() as u64,
            &[
                model_attr.clone(),
                KeyValue::new("gen_ai.token.type", "input"),
            ],
        );
        telemetry.genai_token_usage.record(
            output_tokens,
            &[
                model_attr.clone(),
                KeyValue::new("gen_ai.token.type", "output"),
            ],
        );
        if output_tokens > 0 {
            telemetry.genai_time_per_output_token.record(
                decode_started.elapsed().as_secs_f64() / output_tokens as f64,
                &[model_attr.clone()],
            );
        }
        telemetry.genai_operation_duration.record(
            started.elapsed().as_secs_f64(),
            &[model_attr, KeyValue::new("gen_ai.operation.name", "chat")],
        );

        let text = String::from_utf8_lossy(&output_bytes).trim().to_owned();
        if text.is_empty() {
            bail!("llama generation returned an empty response");
        }

        Ok(text)
    }
}

fn token_bytes(model: &LlamaModel, token: LlamaToken) -> anyhow::Result<Vec<u8>> {
    match model.token_to_piece_bytes(token, INITIAL_TOKEN_BUFFER_SIZE, false, None) {
        Ok(bytes) => Ok(bytes),
        Err(TokenToStringError::InsufficientBufferSpace(required)) if required < 0 => model
            .token_to_piece_bytes(token, required.unsigned_abs() as usize, false, None)
            .map_err(|err| anyhow!("failed to decode llama token bytes: {err}")),
        Err(err) => Err(anyhow!("failed to decode llama token bytes: {err}")),
    }
}
