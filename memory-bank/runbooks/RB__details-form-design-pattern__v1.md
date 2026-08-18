---
type: runbook
version: 1
date: 2025-01-12
tags: [ui, design, forms, thaw, leptos]
---

# Runbook: Details Form Design Pattern

Standard design pattern for read-only detail forms in the Integrator (MPI) project.

## Structure

```
Card (section wrapper)
├── h4.details-section__title (section title)
└── div.form__group (repeated for each field)
    ├── label.form__label (field label)
    └── Input attr:readonly=true (field value)
```

## Grid Layout

For multiple cards:

```rust
<div style="display: grid; grid-template-columns: 600px 600px; gap: var(--spacing-md); max-width: 1250px; align-items: start;">
    <Card attr:style="width: 600px; margin: 0px;">
        // content
    </Card>
    // more cards
</div>
```

## Field Patterns

### Text Field (Read-Only)

```rust
<div class="form__group">
    <label class="form__label">"Field Name"</label>
    <Input value=RwSignal::new(value) attr:readonly=true />
</div>
```

> **Note:** Используем `attr:readonly=true` вместо `disabled=true` - позволяет выделять и копировать текст.

### Boolean Field (Read-Only)

```rust
<Badge
    appearance=BadgeAppearance::Outline
    color=if value { BadgeColor::Success } else { BadgeColor::Danger }
>
    {if value { "Label: Yes" } else { "Label: No" }}
</Badge>
```

### Status Badges

```rust
<Flex gap=FlexGap::Small style="margin-bottom: var(--spacing-md);">
    <Badge appearance=BadgeAppearance::Tint color=BadgeColor::Brand>
        {status_value}
    </Badge>
</Flex>
```

### ID with Copy Button

```rust
#[component]
fn IdWithCopy(value: String) -> impl IntoView {
    let value_for_copy = value.clone();
    view! {
        <Flex gap=FlexGap::Small style="align-items: center;">
            <Input value=RwSignal::new(value) attr:readonly=true attr:style="flex: 1;" />
            <Button
                appearance=ButtonAppearance::Subtle
                shape=ButtonShape::Square
                size=ButtonSize::Small
                on_click=move |_| copy_to_clipboard(&value_for_copy)
            >
                "⧉"
            </Button>
        </Flex>
    }
}
```

### Tables Inside Card

```rust
<Card>
    <h4 class="details-section__title">"Table Title"</h4>
    <Table>
        <TableHeader>
            <TableRow>
                <TableHeaderCell>"Column"</TableHeaderCell>
            </TableRow>
        </TableHeader>
        <TableBody>
            <For each=... />
        </TableBody>
    </Table>
</Card>
```

## CSS Classes Used

| Class                    | Purpose                      |
| ------------------------ | ---------------------------- |
| `details-section__title` | Section header styling       |
| `form__group`            | Field container with spacing |
| `form__label`            | Label styling                |

## Examples

- `a012_wb_sales/ui/details/tabs/general.rs` - 4 cards in 2x2 grid
- `a012_wb_sales/ui/details/tabs/line.rs` - Card with grid fields + Card with table
- `a004_nomenclature/ui/details/tabs/general.rs` - Editable form pattern
