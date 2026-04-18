---
name: slint-best-practices
description: Use when writing, reviewing, or refactoring Slint (.slint) UI files — covers component design, property bindings, callbacks, state management, layouts, accessibility, translations, theming, and integration with Rust backend
---

# Slint Best Practices

Sources:
- [Slint Best Practices (official)](https://docs.slint.dev/latest/docs/slint/guide/development/best-practices/)
- [Slint Custom Controls Guide](https://docs.slint.dev/latest/docs/slint/guide/development/custom-controls/)
- [Slint Language Reference](https://docs.slint.dev/latest/docs/slint/)

---

## 1. Estrutura de Projeto

Separar código, UI e assets em diretórios distintos:

```
my-project/
├── src/        # lógica de negócio (Rust)
├── ui/
│   ├── app-window.slint   # entry point
│   └── components/        # componentes reutilizáveis
└── images/                # SVGs, PNGs, assets
```

**Regra:** Nenhuma lógica de negócio dentro de `.slint`. Slint é declarativo — computações pertencem ao Rust.

---

## 2. Propriedades — Acesso e Direção

Sempre declarar acesso explícito em componentes:

| Modificador | Uso |
|---|---|
| `in` | Dado vem de fora (pai → filho) |
| `out` | Dado vai para fora (filho → pai) |
| `in-out` | Bidirecional (usar com cautela) |
| `private` | Interno ao componente (default) |

```slint
component MyButton {
    in property <string> label;          // pai configura
    out property <bool> pressed;         // pai observa
    private property <bool> hovered;     // interno
}
```

**Evitar `in-out` sem necessidade** — bidirecionalidade cria acoplamento difícil de rastrear.

---

## 3. Bindings Reativos

Bindings se re-avaliam automaticamente quando dependências mudam. **Nunca atribuir manualmente** o que pode ser um binding.

```slint
// ✅ Binding reativo — atualiza automaticamente
Text { text: root.count > 0 ? "Items: \{root.count}" : "Empty"; }

// ❌ Evitar — imperativo, perde reatividade
Text {
    text: "Items";
    // lógica imperativa via callback para atualizar text = ...
}
```

**Regra:** Prefira expressões ternárias e bindings declarativos sobre callbacks que modificam estado.

---

## 4. Callbacks — Direção e Nomenclatura

```slint
component SearchBar {
    callback search-requested(string);    // filho notifica pai
    callback clear-requested();

    // ❌ Evitar: callback que retorna dado para o filho
    // callback fetch-data() -> [DataModel]; // acoplamento invertido
}
```

- Callbacks fluem **de filho para pai** (eventos)
- Dados fluem **de pai para filho** (propriedades `in`)
- Use `<=>` para two-way binding entre propriedades de mesmo nível

---

## 5. Estados e Animações

```slint
component Toggle {
    in-out property <bool> checked;
    private property <brush> bg: #444;

    animate bg { duration: 200ms; easing: ease-in-out; }

    states [
        on when root.checked: { bg: #4CAF50; }
    ]
}
```

- Estados devem ser **mutuamente exclusivos** e baseados em propriedades lógicas
- `animate` deve ser declarado **fora** do bloco `states` (aplica-se à transição)
- Evitar estados baseados em condições negativas — prefira nomes positivos

---

## 6. Layouts

```slint
// ✅ Use componentes de layout semânticos
VerticalBox {
    HorizontalBox {
        Button { text: "Cancel"; }
        Button { text: "OK"; }
    }
}

// ❌ Evitar posicionamento manual com x/y para layouts
Rectangle {
    Button { x: 10px; y: 200px; }  // frágil, não responsivo
}
```

- `VerticalBox` / `HorizontalBox` para layout semântico
- `GridLayout` + `Row` para grids
- Posicionamento absoluto (`x`, `y`) apenas para overlays e elementos decorativos
- Use `preferred-width`/`preferred-height` em vez de valores fixos quando possível

---

## 7. Acessibilidade

Declarar em **todo componente interativo customizado**:

```slint
component CustomButton {
    in property <string> text;
    accessible-role: button;
    accessible-label: self.text;
    accessible-action-default => { clicked(); }
}
```

- `accessible-role` é obrigatório
- `accessible-label` deve ser texto legível por humanos
- Ferramentas: "Accessibility Insights" (Windows), "Accessibility Inspector" (macOS)

---

## 8. Traduções

```slint
// ✅ Correto — permite reordenação pelo tradutor
Text { text: @tr("Hello, {}", name); }

// ❌ Errado — concatenação dificulta tradução
Text { text: @tr("Hello, ") + name; }

// ❌ Esqueceu o @tr
Text { text: "Save Project"; }
```

Toda string visível ao usuário deve usar `@tr("...")`.

---

## 9. Globals

Use `global` para estado compartilhado entre componentes sem prop-drilling:

```slint
export global AppTheme {
    out property <color> accent: #4CAF50;
    out property <length> spacing: 8px;
}

// Uso em qualquer componente
Rectangle { background: AppTheme.accent; }
```

- Globals são singletons — ideal para tema, configurações, estado de app
- Expor globals via `export` para uso no Rust

---

## 10. Integração com Rust

```rust
// Rust: ler propriedade
let val = ui.get_my_property();

// Rust: definir propriedade
ui.set_my_property(42);

// Rust: conectar callback
ui.on_button_clicked(|| { /* handler */ });
```

- Hífens em nomes Slint viram underscores no Rust (`my-prop` → `my_prop`)
- Use **weak references** em closures para evitar ciclos de ownership:

```rust
let ui_weak = ui.as_weak();
ui.on_clicked(move || {
    let ui = ui_weak.upgrade().unwrap();
    ui.set_count(ui.get_count() + 1);
});
```

---

## 11. Imagens com @image-url

`@image-url()` é resolvido em **compile-time** — não aceita strings dinâmicas.

```slint
// ✅ Ternário para seleção condicional
Image {
    source: root.model-id == "amp_a"
        ? @image-url("../assets/amp_a/controls.svg")
        : @image-url("../assets/generic/controls.svg");
}

// ❌ Impossível — @image-url não aceita variável
// Image { source: @image-url(root.model-id + "/controls.svg"); }
```

Para muitos modelos, use if/else encadeado ou componentes separados por tipo.

---

## 12. Nomenclatura

| Elemento | Convenção | Exemplo |
|---|---|---|
| Componentes | PascalCase | `BlockEditorPanel` |
| Propriedades | kebab-case | `block-type-index` |
| Callbacks | kebab-case | `block-selected` |
| Globals | PascalCase | `AppTheme` |
| Estados | kebab-case | `is-hovered` |

---

## 13. Anti-Padrões Comuns

| Anti-padrão | Correto |
|---|---|
| Lógica de negócio em `.slint` | Computar no Rust, expor via propriedade |
| Strings literais sem `@tr` | `@tr("string")` |
| `x`/`y` absolutos para layout | `VerticalBox`/`HorizontalBox` |
| `in-out` desnecessário | `in` ou `out` conforme direção |
| String dinâmica em `@image-url` | Ternários encadeados em compile-time |
| Callbacks que retornam dados | Propriedades `out` para dados, callbacks para eventos |
| Closures Rust sem weak ref | `ui.as_weak()` + `upgrade()` |
