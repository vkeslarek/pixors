# Fase 3: Arquitetura Desacoplada e Renderização via WebAssembly (WGPU)

Este documento descreve o plano arquitetural completo para a Fase 3 do projeto Pixors. O objetivo primordial é migrar da renderização síncrona local com `winit` e CPU (`softbuffer`) para um modelo desacoplado cliente-servidor moderno, utilizando tecnologias da web, WebAssembly (WASM) e aceleração de hardware via `wgpu`.

## 1. Visão Geral da Nova Topologia

A aplicação será dividida em processos/módulos independentes com responsabilidades bem definidas:

1. **`pixors-desktop`**: O shell nativo da aplicação. Uma casca fina construída sobre o [Wry](https://github.com/tauri-apps/wry) (ou Tauri) que inicializa uma WebView para carregar o frontend e gerencia o ciclo de vida do backend.
2. **`pixors-engine` (Backend)**: O "cérebro" pesado do editor. Escrito puramente em Rust, rodará em background e não terá mais a responsabilidade de abrir janelas (fim do uso direto do `winit` para displays). Será um servidor assíncrono.
3. **`pixors-ui` (Frontend)**: Interface gráfica principal criada em React + Vite. Roda dentro da WebView.
4. **`pixors-viewport` (Módulo WASM)**: Um componente crítico compilado de Rust para WebAssembly. Rodará no contexto da aba do frontend gerenciando o elemento HTML `<canvas>` via `wgpu` (WebGPU/WebGL2). Responsável exclusivo por toda a fluidez do pan, zoom e renderização de alta fidelidade na placa de vídeo do usuário.

---

## 2. Padrões de Comunicação (WebSocket)

Como o frontend e o motor agora estarão separados (com o engine possivelmente numa thread nativa de background), toda a comunicação será orquestrada via **WebSocket**. O engine hospedará um servidor (usando `tokio` e `axum`/`warp` ou `tungstenite`).

### 2.1. Canal de Eventos (JSON)
Responsável por eventos da UI e comandos lógicos:
- **UI -> Engine**: Solicitações como "Abra o arquivo X", "Aplique o Filtro Y", "Ajuste o brilho para 1.5", etc.
- **Engine -> UI**: Notificações como "Carregamento concluído: Imagem 4000x3000 carregada", progresso de processamento, histogramas e outras respostas não-bloqueantes.

### 2.2. Canal de Streaming (Binário)
Responsável exclusivamente por enviar fatias de memória da imagem (tiles ou a imagem em resolução reduzida para visualização).
- Ao realizar modificações intensas, o `pixors-engine` envia buffers binários de pixels (`&[u8]`) pelo WebSocket para o `pixors-viewport` (WASM). O WASM, sendo extremamente performático, intercepta os dados, traduz diretamente para a memória do `wgpu` e atualiza a textura no hardware instantaneamente.

---

## 3. O Fim da Renderização por CPU e Início da Era WGSL

Na Fase 2, dedicamos tempo estruturando interpolação Bicúbica e lógica de câmera via matemática de CPU. Isso se provou como um excelente exercício, mas ineficiente para 4K+ em 60 frames. 

### 3.1. Arquitetura do `pixors-viewport` (WASM + WGPU)
- O módulo receberá um ponteiro ou referência de memória apontando para os dados decodificados que vieram via WebSocket.
- O buffer de pixels é carregado como uma `wgpu::Texture`.
- **A Câmera**: Os eventos de scroll (zoom) e arrasto de mouse (pan) **não** vão mais para o motor base. Eles são interceptados em JavaScript (ou no próprio Rust-WASM via `web-sys`), e calculam uma nova *Matriz de Projeção* (Mat4). Essa matriz é enviada para os shaders por meio de um *Uniform Buffer*.
- **Shaders (WGSL)**: O Render Pass de software é substituído por shaders paralelos na GPU:
  - **Vertex Shader**: Recebe vértices de um retângulo 2D (Quad) que mapeia a tela, e aplica a matriz da câmera.
  - **Fragment Shader**: Recebe as coordenadas mapeadas da textura (UV). **A interpolação (Bicúbica/Catmull-Rom) será programada nativamente aqui.** O hardware fará o que demorava centenas de milissegundos em menos de 1ms, proporcionando zoom e pan absolutamente fluidos.

---

## 4. Arquitetura do Novo `pixors-engine`

O Engine se tornará puramente "Headless" (sem interface direta nativa).
- O antigo `viewport.rs` da Fase 2 deixará de interagir com o `softbuffer` e `Swapchain` manual.
- Em vez de re-renderizar para ajustar a visualização do usuário, ele servirá os blocos de dados (`ImageView`) de forma otimizada.
- **Gerenciamento Inteligente**: Se a imagem for massiva (ex: uma raw de câmera de 50 Megapixels), não compensa enviar via WebSocket para a memória do navegador. O Engine poderá aplicar um algoritmo de pirâmides (Mipmapping / Tiling) enviando apenas *Low-Res* para visualização geral da tela cheia, e provendo apenas as subseções focadas em altíssima resolução de acordo com onde a "câmera" da UI está apontando.

---

## 5. Plano de Implementação Progressiva

Para evitar um rewrite doloroso, a Fase 3 será atacada da seguinte maneira:

### Etapa 1: A Fundição WASM
1. Crie o novo subprojeto em Rust: `pixors-viewport`.
2. Configure-o para ser empacotado como biblioteca `cdylib` via `wasm-pack`.
3. Escreva o arcabouço com `wgpu` desenhando um simples triângulo (ou uma textura placeholder com cor estática) para validar se o navegador no Wry está rodando o WebGPU/WebGL.

### Etapa 2: A Ponte de Comunicação
1. Remova o dependência de exibição visual (`winit`/`softbuffer`) do `pixors-engine`.
2. Implemente o runtime `tokio` no motor iniciando o listener de Websocket em uma porta randômica segura.
3. No lado do frontend (React) e/ou do `pixors-viewport` (WASM), inicie a conexão e construa callbacks para tratar pacotes binários e pacotes JSON separadamente.

### Etapa 3: Upload de Texturas WGPU
1. Programe o Engine para codificar uma pequena fatia de imagem estática ou o buffer de pixels base, despachando via WebSocket binário.
2. Programe o `pixors-viewport` WASM para converter a recepção deste payload num `wgpu::Texture` de backend, atrelando a textura no Binding Group para renderização.

### Etapa 4: Câmera Nativa no Frontend e Shaders
1. Implemente as estruturas matemáticas (ex: uso do `glam` ou `cgmath`) dentro do módulo WASM para manter o status de Zoom (`scale`) e Translação (`x`, `y`).
2. Amarre os _listeners_ HTML Canvas no WASM para movimentar e alterar essas posições instantaneamente sem roundtrips com o servidor.
3. Insira o shader em `wgsl` aplicando uma interpolação avançada nos Fragmentos baseada nessa janela de visão flexível.

Ao finalizar essa fase, teremos um produto de classe empresarial: o `engine` pode quebrar, rodar cálculos massivos e pesados sem NUNCA afetar o FPS e a fluidez (60hz/120hz) do pan e zoom que o usuário verá através da tela gerida por WebAssembly e aceleração de Hardware de ponta.
