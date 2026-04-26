# Pixors — Implementation Plan: Phase 7 (UX, ReaEsqueci,ctive Architecture & Desktop Integration)

> Bem-vindo à **Fase 7**. As fundações do motor assíncrono (tiling, websockets, canvas 2D, e mipmaps) já foram estabelecidas e estão funcionais. O foco agora muda do "fazer funcionar" para **"fazer funcionar direito, ser robusto e preparar para o usuário final"**.

Este plano é um guia **altamente detalhado e passo a passo** para o LLM que fará a implementação. Ele contém a sequência exata de tarefas para refatorar o backend, modernizar o fluxo do frontend e envelopar tudo num app Desktop.

---

## Overview da Fase 7

A Fase 7 ataca as maiores dívidas técnicas e gargalos de UX deixados pelo crescimento orgânico da arquitetura. O plano é dividido em 5 etapas estritamente sequenciais:

1. **UX de Loa[IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md)d/Close & Progress Tracking:** Otimizar e dar feedback visual para operações pesadas.
2. **Arquitetura Reativa no Frontend:** Sincronia forte 1:1 entre comandos do front e confirmações do backend.
3. **Fluxos de Exceção (Error Handling):** Exposição clara de erros do motor para a interface de usuário.
4. **Simplificação e Refatoração do Backend:** Limpeza de "spaghetti code", adoção de traits claras e modelos mentais diretos.
5. **Integração Desktop:** Empacotamento do Engine (Rust) + UI (Web) em um executável nativo.

---

## 1. Streamlining de Load/Close & Progress Tracking

O carregamento de arquivos está lento pois exige computação de tiles, downsampling de MIPs inteiros e trocas de perfil de cor. Precisamos tirar o usuário do "escuro".

### Tarefas de Implementação:
- **Backend (Rust):**
  - Adicionar um sistema de emissão de eventos de progresso: `EngineEvent::Progress { task_id: Uuid, percent: f32, message: String }`.
  - No `tab.rs`, dentro do `open_image` e `generate_from_mip0`, disparar eventos incrementais de progresso (ex: "Carregando PNG...", "Gerando MIP 1...", "Gerando MIP 2...").
  - Criar o comando e fluxo reverso de fechamento de abas e do próprio app de maneira limpa (`graceful shutdown`).
- **Frontend (React/TS):**
  - Criar um componente `<ProgressBar />` global (pode ficar sobreposto à tela ou no StatusBar).
  - Interceptar eventos de progresso no `engineClient` e atualizar o estado visual.
  - Bloquear a UI (com overlay leve) se uma tarefa crítica estiver `percent < 100`.
  - Ao concluir, fazer uma transição suave para a imagem (fade-in do Canvas).

---

## 2. Arquitetura Front-End Reativa Estrita

O modelo atual do front é otimista em partes ou state-driven demais, o que causa dessincronia quando o engine está pesado ou travado. O novo modelo será **Command -> Acknowledge -> React**.

### Tarefas de Implementação:
- **Protocolo WebSocket (TS/Rust):**
  - Alterar o `EngineCommand` no frontend para incluir um identificador único de requisição opcional (`req_id: string`).
  - Fazer o backend sempre responder com `EngineEvent::Ack { req_id, status: "ok" | "error" }` assim que terminar a operação principal.
- **Frontend Sync Logic:**
  - Alterar `cmds.sendCommand` para retornar uma `Promise` que aguarda o `Ack` do backend com o `req_id` correspondente (com um timeout de segurança).
  - Mudar as ações de UI (ex: aplicar ferramenta, abrir arquivo) para mostrar estado de "loading" no botão/cursor até a Promise resolver.
  - Travar interações secundárias (desabilitar botões) enquanto aguarda respostas de mutações de estado globais.

---

## 3. Fluxos de Exceção e Tratamento de Erros

Se o backend falha (ex: arquivo corrompido, OOM, acesso negado), o frontend atualmente engole o erro ou printa no console. Precisamos de comunicação explícita.

### Tarefas de Implementação:
- **Backend (Rust):**
  - Garantir que toda e qualquer função falha (`Result::Err`) no roteamento das chamadas retorne um evento `EngineEvent::System(SystemEvent::Error { message, code })`.
  - Para comandos associados a uma aba, enviar também um evento `EngineEvent::Tab(TabEvent::Error { tab_id, message })` para limpar o estado de loading daquela aba.
- **Frontend (React/TS):**
  - Implementar um sistema de Toasts / Notificações nativo na UI usando Radix UI.
  - Escutar globalmente por `SystemEvent::Error` no `useEngineConnection` e disparar o Toast de erro.
  - Se um erro ocorrer durante uma chamada de `sendCommand` com `req_id`, rejeitar a Promise no frontend e mostrar o erro associado.

---

## 4. Simplificação do Backend ("Anti-Spaghetti")

A arquitetura do engine cresceu e os services (`tab.rs`, `viewport.rs`) têm responsabilidades misturadas. É preciso criar "modelos mentais" mais simples.

### Tarefas de Implementação:
- **Mapeamento de Domínios:**
  - Separar estritamente o `AppState` de rotas de WebSocket.
  - Mover lógica massiva do `handle_command` (hoje com centenas de linhas e lógicas inline complexas) para implementações limpas de Traits.
- **Criação de Traits Claras:**
  - Criar a trait `CommandHandler`:
    ```rust
    #[async_trait]
    pub trait CommandHandler {
        type Command;
        async fn execute(&self, cmd: Self::Command, ctx: &mut Context) -> Result<(), AppError>;
    }
    ```
  - Cada comando (ou grupo lógico) deve ser uma struct separada que implementa `CommandHandler`, reduzindo o match gigantesco para roteamento simples.
- **Refatoração de Concorrência:**
  - Limpar os locks. Centralizar estado concorrente em atores gerenciados ou canais, eliminando as condições de corrida complexas vistas nas fases passadas.
  - Padronizar os erros do app em um `AppError` que seja traduzido magicamente para eventos WebSocket de erro (usando `From<Error>`).

---

## 5. Integração Desktop (O Grande Final)

Até agora temos um backend Rust (Engine) e um frontend Vite (React). Eles sobem separados. Para distribuir, precisamos agrupá-los em um único processo Desktop. Dada a stack, **Tauri v2** é a escolha indiscutível.

### Tarefas de Implementação:
- **Setup do Tauri:**
  - Inicializar um crate Tauri na raiz (ex: `pixors-desktop` ou re-aproveitar).
  - Configurar o Tauri para empacotar os assets estáticos gerados pelo `vite build` de `pixors-ui`.
- **Fusão do Engine com Tauri:**
  - Mover a inicialização do Servidor WebSocket Axum para dentro do `tauri::Builder::setup`.
  - O Tauri servirá como host principal. O engine roda em background numa thread Tokio junto do Tauri.
- **Controle Nativo (Window Menu):**
  - Criar menus nativos do SO via Tauri (File -> Open, File -> Close, Window -> Fullscreen).
  - Acionar os comandos de `EngineCommand` (como `OpenFileDialog` ou `CloseTab`) através de invocações diretas do Tauri para o React, ou interceptar comandos nativos e jogar no EventBus local.
- **Deploy Pipeline:**
  - Testar build final unificado (um executável `.exe` / `.AppImage` / `.dmg`) que contém Frontend e Backend trabalhando em conjunto de forma nativa.

---

## Regras para a IA (Implementador)

1. **Siga a Ordem:** Faça a seção 1, depois a 2, etc. Não pule para o Desktop antes de corrigir os fluxos de exceção.
2. **Mantenha os Diff pequenos:** Faça pequenos commits ao resolver cada sub-tópico de uma etapa.
3. **Rust Limpo:** Ao refatorar o backend, use `anyhow` ou `thiserror` corretamente. Se algo puder ser um módulo separado, faça-o.
4. **UX First:** O usuário nunca deve ficar clicando num botão que parece quebrado porque o backend está processando em silêncio. Pense em feedback visual.
