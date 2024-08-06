use rizon_frontend::{
    lexer::Lexer,
    parser::Parser
};
use rizon_tools::results::{Loc, RizonReport, RizonResult};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

#[derive(Debug)]
struct Backend {
    client: Client,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions::default()),
                text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "server initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn completion(&self, _: CompletionParams) -> Result<Option<CompletionResponse>> {
        Ok(Some(CompletionResponse::Array(vec![
            CompletionItem::new_simple("Hello".to_string(), "Some detail".to_string()),
            CompletionItem::new_simple("Bye".to_string(), "More detail".to_string()),
        ])))
    }

    async fn hover(&self, _: HoverParams) -> Result<Option<Hover>> {
        Ok(Some(Hover {
            contents: HoverContents::Scalar(MarkedString::String("You're hovering!".to_string())),
            range: None,
        }))
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;

        self.parse_and_store(uri, text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let changes = params.content_changes;

        // Assuming full sync, otherwise, handle incremental changes
        if let Some(change) = changes.last() {
            self.parse_and_store(uri, change.text.clone()).await;
        }
    }
}

impl Backend {
    async fn parse_and_store(&self, uri: Url, text: String) {
        let mut lexer = Lexer::new();
        let res = lexer.tokenize(&text);

        let tks = match res {
            Ok(tks) => tks,
            Err(errs) => {
                let diags: Vec<Diagnostic> = errs
                    .into_iter()
                    .map(|e| rev_result_to_diagnostic(e, &text))
                    .collect();

                self.publish_diagnostics(uri, diags).await;

                return
            }
        };

        let mut parser = Parser::default();
        let res = parser.parse(tks);

        let diags: Vec<Diagnostic> = match res {
            Ok(_) => vec![],
            Err(errs) => {
                errs
                    .into_iter()
                    .map(|e| rev_result_to_diagnostic(e, &text))
                    .collect()
            }
        };

        self.publish_diagnostics(uri, diags).await;
    }

    async fn publish_diagnostics(&self, uri: Url, diagnostics: Vec<Diagnostic>) {
        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }
}

fn rev_result_to_diagnostic<T: RizonReport>(res: RizonResult<T>, text: &str) -> Diagnostic {
    let loc = res.loc.unwrap_or(Loc::new(0, 0));

    let line_start = &text[..loc.start].chars().filter(|c| *c == '\n').count();
    let start = Position {
        line: *line_start as u32,
        character: loc.start as u32,
    };

    let line_end = &text[..loc.end].chars().filter(|c| *c == '\n').count();
    let end = Position {
        line: *line_end as u32,
        character: loc.end as u32,
    };

    Diagnostic {
        range: Range { start, end },
        severity: Some(DiagnosticSeverity::ERROR),
        message: res.err.get_err_msg(),
        ..Default::default()
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend { client });
    Server::new(stdin, stdout, socket).serve(service).await;
}
