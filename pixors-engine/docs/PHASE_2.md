# Fase 2: Viewport, Swapchain e Interatividade

Este documento especifica a implementação da Fase 2 do projeto, focada em melhorar a exibição de imagens adicionando interatividade (Pan e Zoom) e uma arquitetura robusta de renderização baseada em CPU (`softbuffer`). A intenção é prepararmos o terreno para uma visualização extremamente fluida e escalável.

## 1. Visão Geral da Arquitetura
Atualmente, o projeto copia os pixels diretamente para o `softbuffer` sem tratamento de redimensionamento de janela ou interatividade. A nova arquitetura introduzirá as seguintes abstrações para resolver isso e viabilizar suporte à manipulação de grandes imagens em tempo real:

- **`ImageView`**: Uma abstração (view) que referencia os dados de uma imagem (attachment), desacoplando a posse dos dados da visualização.
- **`ViewRect`**: A "câmera" ou região de visualização. Define qual parte da `ImageView` está sendo exibida e em qual escala matemática.
- **`Swapchain`**: Um gerenciador de buffers circulares em memória com capacidade de múltiplas imagens, garantindo que não haverá *tearing* (quebras no quadro).
- **`Viewport`**: O orquestrador central. Gerencia o swapchain, o contexto da superfície de exibição (`softbuffer`), e o estado atual da visualização (`ViewRect`).

O fluxo principal consistirá em simular um "Render Pass" via software: o Viewport requisita um buffer disponível ao Swapchain, mapeia a `ImageView` requisitada no buffer através de transformações espaciais pelo `ViewRect`, escreve os pixels resultantes e, por fim, solicita o *present* (ou flush) para atualizar a tela nativamente pelo `softbuffer`.

---

## 2. Estruturas de Dados Principais

### `ImageView`
Não deve possuir (own) os bytes da imagem. É uma camada de referência, funcionando como um *attachment* para uma imagem residente na memória do programa (ou mapeada).
- **Semântica**: Age como um "ponteiro" estruturado. Evita cópias desnecessárias de buffers gigantes na memória e facilita o compartilhamento de recursos.
- **Campos e Comportamentos**:
  - Deve conter referências ou slices para os dados brutos dos pixels (ex: `&[u32]`).
  - Metadados essenciais como largura (`width`), altura (`height`), *stride* (passo em bytes ou pixels para avançar uma linha inteira), e detalhes do formato do canal de cor.
  - Provê funções utilitárias ou iteradores que lidam com conversão de coordenadas 2D para índices lineares.

### `ViewRect`
Define a janela lógica de visualização (a câmera) que "olha" para o espaço contínuo da `ImageView`.
- **Campos fundamentais**: `x: f64`, `y: f64` (coordenada de origem da tela projetada no espaço da imagem), e `scale: f64` (fator de escala/zoom).
- **Comportamento**: 
  - **Pan (Arrasto)** = Deslocar as coordenadas `x` e `y` dentro do espaço virtual da imagem.
  - **Zoom (Aproximação)** = Modificar a proporção de `scale`, recalculando dinamicamente a amplitude de área observada.

### `Swapchain`
Mantém um pool de buffers de imagem em memória de tamanho idêntico ao da janela física do SO. Atua como uma fila anelar (circular buffer).
- **Configurabilidade**: Em vez de ser hardcoded para exatamente dois quadros (double-buffer), a arquitetura do Swapchain deve aceitar em sua instanciação o número `N` de buffers. Isso oferece flexibilidade vital para testar esquemas como *triple-buffering* visando otimizar latências na troca de frames.
- **Estrutura Circular**:
  - Coleção de `N` buffers unidimensionais (ex: `Vec<Vec<u32>>`), onde cada um tem comprimento `width * height`.
  - Ponteiros de controle de estado: indica qual índice é o alvo de gravação atual e qual índice está disponível como frente para apresentação visual.
- **Métodos Exigidos**:
  - `acquire_next_image()`: Tenta obter e reservar o próximo slot circular livre para uso pela CPU durante a renderização, travando seu estado.
  - `present()`: Conclui o acesso de renderização, avançando o ponteiro de exibição para esse buffer recién desenhado.
  - `resize(new_width, new_height)`: Descarta imediata e graciosamente as alocações contíguas existentes e reinicializa todo o conjunto de `N` buffers com o novo limite de resolução do OS.

### `Viewport`
A engrenagem mestre responsável por organizar o pipeline.
- **Responsabilidades de Estado**:
  - Tem controle mutável sobre a superfície nativa (a janela referenciada do `softbuffer`).
  - Retém o ciclo de vida e configuração do seu `Swapchain`.
  - Encapsula o `ViewRect` ativo.
- **Rotinas Chave**:
  - **Resize**: Escuta callbacks de mudança de área, propagando-os para o hardware buffer (atualizando sua extensão) e instruindo o Swapchain para realocação.
  - **Render Pass (`render(&self, image: &ImageView)`)**: Executa uma passagem de renderização: tranca um quadro pelo `Swapchain::acquire_next_image()`, executa amostragens massivas iterando de acordo com as especificações matemáticas do `ViewRect` e assinala que a imagem terminou de montar com `present()`.
  - **Flush/Apresentação**: Realiza o dump veloz do último quadro preenchido integralmente para o monitor, copiando a memória para o framebuffer final no window-manager.

---

## 3. Algoritmos de Renderização e Interpolação

Como as medidas matemáticas raramente estão de acordo entre os pixels da sua tela e a escala da imagem desejada, não copiamos fatias de memória indiscriminadamente. **Amostramos (sample)** a `ImageView`.

### Mapeamento (Tela -> Imagem)
Para cada pixel discreto na coordenada da sua tela física `(tx, ty)` no buffer atual do Swapchain:
1. Mapeie para uma coordenada espacial contínua `(ix, iy)` correspondente da respectiva imagem original `ImageView`. A matemática embute inverter as funções de escala e translação contidas no `ViewRect`.
2. Resolva a cor necessária baseando-se naquele ponto fracionário aplicando o kernel de **Interpolação**.
3. Assine aquele valor em formato embalado de volta no buffer final (como `XRGB`).

### Otimização Rigorosa: Processamento Vetorial (SIMD)
Construir quadros percorrendo pixels com loops `for` tradicionais escalares rapidamente devorará todos os recursos das CPUs. A regra de ouro dessa camada é de que **o uso de aceleração por hardware vetorizada (SIMD) é indispensável e compulsório**. O loop base interior não será paralelizado usando multi-threading superficial (como a biblioteca *Rayon*), e sim otimizado via vetores (através de crates como `std::simd` no nightly ou alternativas em stable como `wide`). Devem-se processar janelas contendo blocos de 4 a 8 pixels concomitantemente durante a amostragem, garantindo performance e responsividade formidáveis no thread principal.

### Interpolação Cúbica (Bicubic)
Nível alto de fidelidade no Zoom, dispensando a mediocridade visual e os borrões causados por mapeamento simples Bilinear.
- Consiste em analisar a vizinhança matricial de 4x4 pixels envolta do ponto decimal fracionário `(ix, iy)`.
- É aplicada uma curva peso (Cúbica/Catmull-Rom) ponderando cada intensidade baseada na distância do pixel nativo em relação à coordenada decimal exata requisitada.

---

## 4. Dinâmica de Eventos e Interatividade (`winit`)

Devemos injetar callbacks na fila unificada de processamento (`EventLoop`) do `winit`, conectando a semântica da câmera da Fase 2.

### Pan (Movimentação por Arrasto)
- **Inputs Associados**: Preste atenção ao evento `WindowEvent::MouseInput` (para estado botões: Down vs Up) combinados com interações de transição em `WindowEvent::CursorMoved`.
- **Implementação**: 
  - Monitore quando o mouse afunda e registre os eixos (`tx, ty`).
  - Cada frame onde ele continuar afundado disparando em um novo `CursorMoved`, subtraia ou adicione o delta traduzindo essas proporções dividindo pelo fator do Zoom. Mute diretamente a estrutura `ViewRect`.
  - Dispare o marcador de sujeira de UI (dirty flag) solicitando uma re-execução do Render Pass.

### Zoom (Escala)
- **Inputs Associados**: Intercepte os pacotes emitidos por `WindowEvent::MouseWheel` (scrolling convencional) ou detecção capacitiva baseada em `TouchpadMagnify`.
- **Lógica de Âncora Perfeita**:
  - Evite atalhos ingênuos. Se você apenas multiplicar `scale` do `ViewRect`, a aproximação ocorrerá em direção às coordenadas fixas do canto superior ou central da visualização (causando uma forte desorientação em quem usa).
  - Capture em que coordenada exata contínua o mouse se encontrava descansando acima da imagem nos micro-segundos anteriores ao rolamento.
  - Altere a intensidade matemática `scale`.
  - Imediatamente engate uma correção ajustando as variáveis `x` e `y` do `ViewRect` em compensação para que aquela mesma coordenada se mantenha cravada por baixo da ponta estática do ponteiro do cursor durante o crescimento na escala.

### Resize da Janela
- **Inputs Associados**: Respostas nativas a mutação geométrica dadas via `WindowEvent::Resized`.
- Apenas alimente este sinal novo para que a lógica de realocação ocorra sobre os pools flexíveis alocados do `Swapchain` (re-solicitando buffers nativos em arrays novos limpos e recriando as extensões do backend `softbuffer`). Preserva a escala e o foco intactos após expandir.

---

## 5. Passos de Implementação Progressiva

1. **Camada Lógica**: Desenvolva o esqueleto para os tipos `ImageView` (como attachment/slice read-only), `ViewRect`, e `Swapchain` como um array iterável com `N` slots de buffer de profundidade e rotação circular.
2. **Integração Básica (`Viewport`)**: Conecte o modelo Swapchain às raízes do loop nativo de SO (`Resized`). Faça hard-tests emissores de cores chapadas em tela que rotacionem frames sem falhas de transição usando flush via a camada já inserida de `softbuffer`.
3. **Eventos `winit` (Input)**: Monte e conecte explicitamente as capturas físicas como botões de mouse, cursores movendo e giro do rolamento (Scroll View). Acople a essas as lógicas exigidas de cálculos matemáticos de compensação listadas em Dinâmicas de Eventos.
4. **Mapeamento Primitivo e Render**: Aplique a função inicial limpa da translação Câmera para Display. Renderize uma amostragem básica rápida usando vizinho-mais-próximo (*Nearest Neighbor*).
5. **Alta Fidelidade Extrema (Bicubic + SIMD)**: Finalize mergulhando no núcleo matemático de processamento visual restrito de cada quadro. Elimine o gargalo single pixel por pixel em prol de vetores computados em lotes paralelos (`SIMD intrínseco`) e instaure o peso do kernel de vizinhança matricial (4x4 de Bicubic) sem estourar alocações.
