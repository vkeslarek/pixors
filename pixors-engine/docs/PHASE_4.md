# Fase 4: Otimização de Processamento, Tiling e Gerenciamento de Sessão

## 1. Visão Geral
A Fase 4 marca a maturação do `pixors-engine` no que tange a gerenciamento de memória e processamento. Atualmente, quando uma imagem é carregada, os dados fluem sincronamente e o motor tenta transacionar imagens pesadas inteiras de uma só vez para o frontend. Isso é um gargalo inadmissível em ambientes de edição profissional.

Esta fase introduz uma arquitetura robusta para streaming de dados progressivo, desvinculação de recursos de memória física da abstração da imagem e isolamento de estado baseado em Sessões (Sessions).

**Objetivos primários:**
- **Sessões Isoladas**: O sistema não será mais um singleton de estado (`AppState` fixo numa única imagem). Os recursos passam a viver dentro de uma **Session**. Quando a aba do editor fecha ou a sessão é invalidada, tudo é liberado agressivamente do hardware/RAM.
- **Tiling (Quebra em Tiles)**: A estrutura fundamental de manipulação deixa de ser um único *blob* enorme. A imagem passa a ser fatiada conceitualmente em **Tiles** (blocos).
- **Desvinculação do Storage**: Ao abrir um PNG de 50MB, os bytes originais formam o *Storage*. A abstração visual da `Image` deve referenciar esse Storage através dos Tiles via ponteiros lógicos ("retiling"), evitando cópias de memória profundas e inúteis (Zero-copy logical tiling).
- **Viewport Extremamente Passivo ("Dumb")**: O `pixors-viewport` perde autonomia. Ele não processa ou entende imagens; ele só recebe pacotes de *Tiles* e "cola" na textura exibida usando a GPU. Em contrapartida, ele emite eventos de mouse/UI para o Engine.
- **Ações de UI Base**: Criar o clássico *Menu Arquivo > Abrir* no React e conectá-lo ao novo fluxo de sessões na API REST do Engine.

---

## 2. Nova Topologia: API REST + WebSocket de Sessão

O motor atualiza seu paradigma. O WebSocket não recebe mais comandos monolíticos soltos, a orquestração passa para o padrão HTTP REST para controle de Estado e WebSocket para Alta Frequência / Streaming.

### 2.1 Fluxo Arquitetural Desejado
1. **Boot da UI**: O usuário abre o app. A UI faz `POST /api/session` e ganha um `session_id` (ex: UUID).
2. **Setup do Canal**: O React abre o Viewport WASM e esgota a conexão no WebSocket do Engine apontando para aquela sessão: `ws://127.0.0.1:8080/ws?session_id=...`.
3. **Arquivo > Abrir**: O usuário clica em *Abrir* e escolhe a imagem. O React envia a instrução ao backend: `POST /api/file/open { session_id, path }`.
4. **Alocação Lógica**: O Rust carrega a imagem em memória e aloca os recursos dentro do dicionário da *Sessão*.
5. **Streaming Rápido**: Imediatamente, o Engine sabe como fazer o Tiling e começa a emitir os tiles da imagem um a um pelo WebSocket aberto no passo 2. O Viewport renderiza magicamente tile a tile.
6. **Desmonte**: Se o usuário fechar ou mudar de arquivo, dispara `DELETE /api/session/:id`. Toda a matriz de memória é destruída (Drop no Rust).

---

## 3. Conceituação e Implementação de Tiling

A desvinculação entre "Armazenamento Lógico" (Memory) e "Armazenamento Visual" (Tiles) é o motor da FASE 4.

### 3.1 Desvinculando `ImageRaw`
- A leitura do disco continuará retornando a imagem crua, residente de forma plana na RAM.
- Criaremos uma estrutura chamada **`Tile`**. Ela conterá:
  - `x`, `y` absolutos na imagem original.
  - `width`, `height` (ex: chunks de 256x256).
  - Um offset (ponteiro lógico ou View) apontando para o trecho do `ImageRaw` responsável.

### 3.2 Retiling Virtual e Zero-Copy
- Retiling é a operação que ocorre logo que um PNG de "1 tile gigante" termina de ser lido.
- Em vez de copiarmos os bytes pra formar um grid 2D rígido na memória, fazemos um fatiamento apenas por abstração matemática (índices). 
- Assim, alocar milhares de tiles não consome um byte sequer a mais de RAM, apenas alguns bytes de metadados (`[x, y, w, h]`).

---

## 4. O "Dumb Viewport" (Viewport Estúpido)

O `pixors-viewport` deve ser refatorado para ter o menor número possível de regras de negócio. Ele é apenas um *receptor* de pintura.

### 4.1 Papel do Viewport (WASM/React)
- **Input de Mosaico**: A API do WASM passa a expor uma função simples como `pixors_write_tile(x, y, w, h, binary_data)`. 
- **Pintura Otimizada (GPU)**: O núcleo WGPU roda `queue.write_texture(...)` jogando apenas aquele sub-retângulo (`w`, `h`) na exata coordenada (`x`, `y`) da Swapchain/Textura. Sem re-renderizar a tela toda.
- **Responsabilidade do Engine**: O motor envia a mensagem através do WebSocket: `"type": "tile", "x": 0, "y": 0, ...` seguida do *buffer* contendo os pixels do tile.
- **Gestão de Pan/Zoom**: Se houver Pan/Zoom, a UI atualiza sua matrix local, avisa o servidor, e o servidor (eventualmente na Fase 5) recalcula se há novos tiles a serem lidos da memória para aquele enquadramento e os retransmite.

### 4.2 Emissão Tile-a-Tile Assíncrona
- Em vez do Engine bloquear a thread esperando a serialização do vetor inteiro de 90MB da imagem:
- Ele fará um loop assíncrono emitindo `ServerEvent::TileData` para os tiles visíveis no viewport inicial.
- O efeito é de um *progressive render* natural, extremamente fluído, eliminando o *freeze* do front-end e melhorando substancialmente a UX.

---

## 5. Checklist Técnico para o Implementador (LLM)

Siga os requisitos estritos abaixo. *Não adivinhe* requisitos da fase 5. Atenha-se a executar o Tiling e Session Management com maestria!

1. **REST API de Sessão**:
   - Atualize `router.rs` e `state.rs`.
   - Adicione rotas: `POST /session`, `GET /session/:id`, `DELETE /session/:id`, `POST /file/open`.
   - O `AppState` deve agora gerenciar um dicionário concorrente (ex: `RwLock<HashMap<Uuid, SessionState>>`).
2. **Estruturas de Tile (`src/image/tile.rs` ou similar)**:
   - Crie as definições de Tile com lógica *Zero-Copy* apontando para a Imagem Base.
   - Crie uma função eficiente de "Retiling" paramétrico (ex: gerar tiles de 256x256).
3. **Modificação do Protocolo WebSocket**:
   - Crie o evento `ServerEvent::TileData { x, y, width, height, size }`.
   - Modifique o Engine para iterar no Grid de Tiles e emitir as fatias isoladamente para o socket ativo daquela Sessão.
4. **Otimização do WGPU / Viewport**:
   - Altere o `pixors-viewport` em Rust/WASM para conter o método de injeção de Tiles na textura.
   - Remova responsabilidades analíticas e de negócio do código WASM.
5. **Integração Visual (UI)**:
   - Construa um menu falso nativo em HTML/CSS ("Arquivo" > "Abrir") que chame a API REST e gerencie o UUID da sessão na aba atual.
   - O UI escutará os eventos `TileData` do WebSocket e chamará o motor WGPU para escrever o mosaico progressivamente.
