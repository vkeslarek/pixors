# Fase 3: Arquitetura Desacoplada e Renderização via WebAssembly (WGPU)

Este documento descreve o plano arquitetural completo para a Fase 3 do projeto Pixors. O objetivo primordial é migrar da renderização síncrona local com `winit` e CPU (`softbuffer`) para um modelo desacoplado cliente-servidor moderno, utilizando tecnologias da web, WebAssembly (WASM) e aceleração de hardware via `wgpu`.

## 1. Visão Geral da Nova Topologia

A aplicação será dividida em processos/módulos independentes com responsabilidades bem definidas:

1. **`pixors-desktop` (Já Criado!)**: O shell nativo da aplicação. Uma casca fina construída sobre o [Wry](https://github.com/tauri-apps/wry) que inicializa a WebView para carregar o frontend e gerencia o processo em background do backend.
2. **`pixors-engine` (Backend)**: O "cérebro" pesado do editor. Escrito puramente em Rust, rodará em background e não terá mais a responsabilidade de abrir janelas (fim do uso do `winit` e `softbuffer`). Será essencialmente um servidor assíncrono headless com processamento vetorizado (SIMD).
3. **`pixors-ui` (Frontend)**: Interface gráfica interativa criada em React + Vite. Roda dentro da WebView.
4. **`pixors-viewport` (Módulo WASM)**: Um componente crítico compilado de Rust para WebAssembly. Rodará no contexto do frontend (como um pacote importado), gerenciando o elemento HTML `<canvas>` utilizando a API do `wgpu`. Responsável exclusivo por toda a fluidez do pan, zoom e apresentação visual interativa.

---

## 2. Padrões de Comunicação (WebSocket)

Toda a comunicação entre o frontend e o motor de background será orquestrada via **WebSocket** (usando bibliotecas como `tokio` e `tungstenite` no backend).

### 2.1. Canal de Eventos (JSON)
Responsável por eventos da UI e comandos lógicos:
- **UI -> Engine**: Solicitações como "Abra o arquivo X", "Aplique o Filtro Y".
- **Engine -> UI**: Notificações e metadados como "Carregamento concluído: Imagem 4000x3000 carregada".

### 2.2. Canal de Streaming (Binário)
Responsável exclusivamente por enviar fatias de memória da imagem:
- O `pixors-engine` enviará buffers binários puros de pixels (`&[u8]`) pelo WebSocket para o `pixors-viewport`. O WASM, com sua performance nativa, extrai esses pacotes, converte e envia diretamente para a memória de vídeo via `wgpu`.

---

## 3. Aceleração de Hardware Descomplicada (Texture e Swapchain)

Na Fase 2, programamos lógica de interpolação e câmera diretamente na CPU. Isso foi vital para o processamento "offline" do backend. Contudo, para *apresentar* o pan e o zoom interativo ao usuário em 60fps+, vamos transferir essa carga para a placa de vídeo de maneira simplificada.

### 3.1. Apenas `wgpu::Texture` e Samplers Nativos
- **Não perderemos tempo escrevendo cálculos complexos nos shaders (WGSL)** como *Render Passes* super customizados ou matemática bicúbica manual no *Fragment Shader*.
- A arquitetura consiste primariamente em pegar os bytes recebidos, alimentar uma `wgpu::Texture` com eles, e utilizar as funcionalidades robustas e integradas do próprio hardware gráfico para projetá-los para o `Swapchain` da janela.
- Filtros de *minification* e *magnification* (como linear ou anisotrópicos, que simulam visuais suaves de interpolação) são geridos pelos *Samplers* nativos do `wgpu` por padrão, tornando pan e zoom fluidos com altíssimo desempenho a custo zero de processador.

### 3.2. Separação Estrita de Paradigmas Matemáticos (Atenção!)
A forma de calcular matemática gráfica para a GPU e a forma de otimizar a CPU são diferentes neste projeto!
- **Frontend (`pixors-viewport`)**: Como lidará com matrizes de câmera (Mat4) para o Viewport no WebAssembly, você estará autorizado a usar bibliotecas gráficas padrão de álgebra linear (como o `glam`). Elas são excelentes para controlar translação e escala (Pan/Zoom).
- **Backend (`pixors-engine`)**: **Não deve usar** o `glam` ou utilitários focados em *graphics API*. Como todo o pipeline de processamento do Editor roda intensamente na CPU, ele já conta com uma estrutura matricial e matemática própria. Isso é vital para garantir que os cálculos do núcleo gráfico explorem todo o potencial das instruções vetorizadas nativas da CPU (**SIMD** - *Single Instruction, Multiple Data*).

---

## 4. O Novo `pixors-engine` (Headless Server)

O Engine abandona de vez a preocupação sobre como "pintar" na tela.
- O antigo setup visual deixará de interagir com buffers de janela de OS local.
- Ele se tornará puramente uma API reativa que escuta ações e serve blocos decodificados (`ImageView`).
- O foco computacional dele fica em: Decodificação, Colorimetria, Conversões pesadas e preparo dos Pixels, enviando o prato pronto para a Interface comer.

---

## 5. Plano de Implementação Revisado

### Etapa 1: Limpeza e Setup do Servidor Headless
1. Reconheça que a casca (`pixors-desktop`) já está criada e estruturada.
2. Remova definitivamente as dependências visuais residuais (`winit`, `softbuffer`) do interior do `pixors-engine`.
3. Incorpore um runtime assíncrono (`tokio`) e levante o WebSockets server (`tungstenite` ou `axum`) para que o engine rode como serviço de background.

### Etapa 2: A Fundição WASM
1. Crie o pacote Rust da Interface Gráfica: `pixors-viewport`. Configure-o via `wasm-pack` para expor bindings ao JavaScript/TypeScript.
2. Insira as conexões WebSockets para parear com a porta do `pixors-engine`.
3. Defina um receiver em binário e um event loop que assimile os payloads no WASM.

### Etapa 3: Integração Minimalista do WGPU
1. Puxe a inicialização de um *Canvas* atrelando a superfície nativa da GPU usando as estruturas padrões de um boilerplate básico de `wgpu`.
2. Pegue os arrays binários que chegam do backend e alimente diretamente uma instância contínua de `Texture`.
3. Assine essa Textura na Swapchain final, ativando amostragem suave sem estender desnecessariamente pipelines de Renderização, deixando a API do WGPU orquestrar o envio para a tela de maneira simplista.

### Etapa 4: Câmera com `glam`
1. Adicione a dependência do `glam` **exclusivamente** na raiz do projeto `pixors-viewport`.
2. Intercepte `onWheel` (scroll) e `onPointerMove` (arrasto) do React/Canvas em TypeScript e passe os deltas para o Rust WebAssembly.
3. Componha as novas transformações em uma Matriz, transladando os limites de leitura da Textura no WGPU e atingindo navegação instantânea pelo espaço da imagem.
