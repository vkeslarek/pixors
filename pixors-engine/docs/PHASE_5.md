# Phase 5: UX & Desktop Overhaul (Professional Look & Feel)

## Visão Geral
A Fase 5 muda o foco da infraestrutura de backend (motor) para a experiência do usuário (Front-end). O objetivo é elevar o `pixors-ui` e o `pixors-desktop` para que tenham a aparência, a sensação e o comportamento de uma aplicação de edição de imagem profissional e premium (como Photoshop, Figma, Affinity Photo). Esta fase estabelecerá um Design System robusto, implementará estruturas avançadas de layout de software e fará o polimento do shell da aplicação desktop.

---

## 1. Design Tokens & Theming System
- **Paleta de Cores:** Definir uma paleta de *Dark Mode* profissional (ex: cinzas neutros e escuros profundos, com cores de destaque sutis para estados ativos). O foco é não cansar a vista do profissional.
- **Tipografia:** Implementar uma família de fontes limpa e de alta legibilidade (ex: *Inter*, *Roboto* ou *San Francisco*) com escalas definidas para a interface.
- **Spacing & Sizing:** Padronizar paddings, margins e alturas de componentes para garantir uma UI densa, porém clicável e legível.
- **Implementação:** Criar arquivos de tokens (`tokens.css` ou tema central no Tailwind/Styled Components) no `pixors-ui` para atuar como a única fonte da verdade (Single Source of Truth).

## 2. Biblioteca de Componentes (Component Library)
- **Adoção de UI Headless:** Importar e configurar uma biblioteca de componentes pré-prontos e acessíveis (como **Radix UI**, **shadcn/ui** ou **MUI**) para acelerar o desenvolvimento de interfaces complexas sem perder o controle do visual.
- **Componentes Essenciais a serem implementados:**
  - **Dropdown Menus:** Para a barra de menu superior (Arquivo, Editar, Visualizar, Imagem, Janela).
  - **Tooltips:** Cruciais para os botões da barra de ferramentas (que terão apenas ícones), ajudando na descoberta das funções.
  - **Context Menus:** Menus de clique com botão direito nativos da aplicação (desabilitando o menu padrão do navegador).
  - **Modals/Dialogs:** Para configurações, exportação e popups de aviso.

## 3. Arquitetura de Layout da Aplicação
- **App-like Layout (CSS Grid/Flexbox):** Reestruturar o layout principal para ser rígido e responsivo. A aplicação **não pode ter scroll na tag body** (apenas scroll interno nos painéis).
- **Regiões do Layout:**
  - **Top Menu Bar:** Barra superior com menus dropdown no estilo nativo.
  - **Left Toolbar:** Barra vertical de ferramentas (Mover, Seleção, Pincel, Pan) com estados de ativação visíveis.
  - **Right Sidebar (Sticky Panels):** Painéis empilháveis, colapsáveis e "sticky" para Camadas (Layers), Propriedades, Color Picker e Histórico.
  - **Bottom Status Bar:** Barra inferior fina para mostrar nível de Zoom, dimensões da imagem, perfil de cor (ex: sRGB) e consumo de RAM.
  - **Central Viewport:** A área do canvas infinito onde a imagem será renderizada.

## 4. Overhaul do Desktop Shell (`pixors-desktop` / Wry)
- **Identidade da Aplicação (Branding):**
  - Gerar e integrar o novo logo da aplicação (ícones `.ico` para Windows, `.icns` para macOS, `.png` para Linux).
  - Atualizar o nome da aplicação para "Pixors" e os metadados de construção nativos.
- **Gerenciamento de Janela (Window Management):**
  - Definir dimensões padrão para uma área de trabalho profissional (ex: `1280x800` como tamanho mínimo, `1440x900` como padrão).
  - Habilitar barra de título customizada (Frameless/Mac-like) se possível, para integrar os controles de janela na Top Menu Bar.
- **Bloqueios de Navegador:**
  - Desabilitar a seleção de texto na UI (`user-select: none`).
  - Prevenir o clique direito padrão da web (`contextmenu.preventDefault()`).
  - Prevenir comportamentos de "arrastar" imagens indesejados.

## 5. Viewport UX & Interações (Canvas Infinito)
- **Navegação de Alta Performance:** Integrar perfeitamente o WGPU Viewport com eventos do mouse para permitir *Pans* (arrastar com o botão do meio ou barra de espaço) e *Zooms* suaves (Scroll-wheel).
- **Rulers & Grid:** (Opcional para esta fase) Preparar o espaço visual para réguas nas bordas e grid de pixels quando o zoom ultrapassar 800%.
- **Cursores Dinâmicos:** Mudar o cursor do mouse baseado na ferramenta selecionada (ex: `grab` / `grabbing` para a ferramenta Hand, `crosshair` para seleção).

## 6. Micro-interações e Polimento (Look and Feel)
- **Estados Visuais:** Adicionar feedback visual imediato (`hover`, `active`, `focus`) a todos os botões e ferramentas.
- **Transições:** Garantir que a abertura/fechamento de painéis seja rápida, fluida e acelerada por hardware (`transform` / `opacity`).
- **Keyboard Shortcuts:** Implementar gerenciador global de atalhos (ex: 'V' para mover, 'H' para Pan, Ctrl+O para abrir).

---

## Passos de Implementação (Para a próxima IA)

1. **Configuração Inicial do Wry:** Edite a inicialização da janela Wry no desktop (`pixors-desktop/src/main.rs`) ajustando título, bloqueio de tamanho mínimo e travas de UI. Adicione e associe os assets de Logo gerados para o binário.
2. **Tokens e Estilos Base:** Limpe o CSS antigo do React e implemente os Design Tokens no `pixors-ui`. Adicione reset de estilos (`user-select: none`, `overflow: hidden` no body).
3. **Instalação da Biblioteca de UI:** Instale o shadcn/ui ou Radix (ou similar) e inicie os componentes base (Menu, Tooltip).
4. **Construção do Skeleton:** Monte a estrutura Grid com TopBar, Toolbar Esquerda, Sidebar Direita e Viewport Central.
5. **Integração do Viewport:** Coloque o componente React do WGPU Viewport ancorado perfeitamente na área central e conecte os eventos de atalhos de teclado.
6. **População de Menus e Ferramentas:** Adicione os ícones (Lucide, Phosphor, ou Heroicons) na Toolbar e crie a árvore de menus da barra superior.

Este documento guiará a próxima iteração para transformar o Pixors de um protótipo técnico em um produto de software profissional e altamente usável.
