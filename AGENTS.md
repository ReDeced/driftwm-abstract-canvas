# AGENTS.md — инструкция для агентов

Этот репозиторий — `driftwm-abstract-canvas` (пакет `driftwm-wallpaper`), внешняя система обоев для композитора [driftwm](https://github.com/roman/driftwm) через IPC. Приложение отрисовывает абстрактные обои на Wayland-поверхности layer-shell (фоновый слой) с помощью wgpu и передаёт картинку через SHM.

Ниже — руководство, как добавлять новые абстрактные модули обоев и править существующий код.

---

## 1. Структура проекта

```
Cargo.toml               # манифест Crate (имя пакета: driftwm-wallpaper)
Makefile                 # make / make install / make test / make fmt
src/main.rs              # точка входа: CLI, цикл событий Wayland + IPC
src/lib.rs               # экспорты модулей + юнит-тесты IPC-протокола
src/config.rs            # загрузка TOML-конфига (AppConfig, модули, слой, рендеринг, тема)
src/ipc.rs               # типы протокола driftwm (Request / Response / StateInfo / Event)
src/ipc_client.rs        # клиент Unix-сокета IPC (подключение, запросы, подписка на события)
src/layer_shell.rs       # Wayland layer-shell поверхность с SHM-презентацией (smithay-client-toolkit)
src/renderer.rs          # wgpu-рендерер (offscreen-текстура, чтение на CPU через staging buffer)
src/wallpaper.rs         # WallpaperManager + трейт WallpaperModule + все модули обоев
src/shaders/*.wgsl       # WGSL-шейдеры для каждого модуля
tests/integration_test.rs# интеграционные тесты конфига и сериализации IPC
config.toml.example      # пример конфигурации
```

## 2. Как добавить новый модуль обоев

Каждый модуль — это реализация трейта `WallpaperModule` в `src/wallpaper.rs`. Шаги:

### 2.1. Создай шейдер `src/shaders/<name>.wgsl`

Шейдер должен содержать:
- `struct Uniforms` с данными, которые передаются из Rust (`resolution`, `camera`, `zoom`, `time`, `pan_angle`, `pan_rate`, `palette_colors`, `palette_size`, …).
- `@group(0) @binding(0) var<uniform> uniforms: Uniforms;`
- `vs_main` — полноэкранный quad (triangle strip).
- `fs_main` — возвращает `@location(0) vec4<f32>`.

Пример — сферическая геометрия с положительной кривизной: `src/shaders/spherical.wgsl`.

### 2.2. Добавь конфигурацию в `src/config.rs`

Структура конфига модуля должна реализовывать `Default` и `serde::Deserialize`. Добавь её в `ModuleConfigs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModuleConfigs {
    #[serde(default)]
    pub hyperbolic: HyperbolicConfig,
    #[serde(default)]
    pub solid: SolidConfig,
    #[serde(default)]
    pub image: ImageConfig,
    #[serde(default)]
    pub spherical: SphericalConfig,   // <- новый модуль
    #[serde(default)]
    pub <твой_модуль>: <ТвойConfig>, // <- ещё один модуль
}
```

Также добавь секцию в `config.toml.example` и документацию в README.

### 2.3. Реализуй модуль в `src/wallpaper.rs`

1. Добавь `use crate::config::{..., <ТвойConfig>};` в начале файла.
2. Создай структуру модуля с полями: `pipeline: Option<wgpu::RenderPipeline>`, `bind_group: Option<wgpu::BindGroup>`, `uniform_buffer: Option<wgpu::Buffer>`, `width`, `height`, `pan_angle`, `palette`, и т.д.
3. Реализуй `impl WallpaperModule for <ТвойМодуль>`:

```rust
impl WallpaperModule for <ТвойМодуль> {
    fn new(modules: &ModuleConfigs) -> Result<Self> {
        Self::new(&modules.<твой_модуль>) // вызывает свой конструктор
    }

    fn update(&mut self, frame: &FrameData) {
        // обновление внутреннего состояния (например, pan_angle)
    }

    fn pipeline(&self) -> &wgpu::RenderPipeline {
        self.pipeline.as_ref().unwrap()
    }

    fn bind_group(&self) -> &wgpu::BindGroup {
        self.bind_group.as_ref().unwrap()
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.pipeline = None; // форсирует пересоздание при следующем рендере
    }

    fn reload_config(&mut self, modules: &ModuleConfigs) -> Result<()> {
        // перечитать конфиг, сбросить pipeline
        Ok(())
    }

    fn name(&self) -> &str {
        "<имя_модуля>"
    }
}
```

4. Добавь ветку в `WallpaperManager::create_module`:

```rust
"<твой_модуль>" => Ok(Box::new(<ТвойМодуль>::new(modules)?)),
```

### 2.4. Положительная кривизна (сфера) vs отрицательная (гиперболика)

- `hyperbolic.wgsl` — диск Пуанкаре, отрицательная кривизна, клетки расходятся к краю диска.
- `spherical.wgsl` — ортографическая проекция сферы, положительная кривизна, клетки сходятся к полюсам, пространство замыкается на себя.
- Выбирай модуль в конфиге: `module = "spherical"` или командой `-m spherical`.

---

## 3. Работа с конфигурацией

Конфиг загружается из (в порядке приоритета):
1. `~/.config/driftwm-abstract-canvas/config.toml` (или путь из `-c`)
2. `/etc/driftwm-abstract-canvas/config.toml`
3. Переменные окружения с префиксом `DRIFTWM_WALLPAPER_`

Пример (`config.toml.example`):

```toml
module = "hyperbolic"   # | "solid" | "image" | "spherical"

[modules.spherical]
radius = 1.0
pan_rate = 0.15
palette = { type = "named", value = "sphere" }
interactive = true

[layer]
namespace = "driftwm-abstract-canvas"
z_order = -100
```

## 4. Как запускать и тестировать

```bash
# Сборка
cargo build --release
# или
make

# Тесты (все должны проходить)
cargo test
# или
make test

# Проверка компиляции
cargo check
# или
make check

# Форматирование
cargo fmt
# или
make fmt

# Установка бинарника
make install          # по умолчанию в /usr/local/bin/driftwm-abstract-canvas
make uninstall

# Запуск (требует сессию driftwm под Wayland)
./target/release/driftwm-abstract-canvas
./target/release/driftwm-abstract-canvas --debug
./target/release/driftwm-abstract-canvas -m spherical
./target/release/driftwm-abstract-canvas --dump-state
```

## 5. Как интегрировать с driftwm

В конфиге driftwm (`~/.config/driftwm/config.toml` или эквивалент) укажите:

```toml
autostart = [
    "driftwm-abstract-canvas",
]

[background]
type = "none"
```

- `autostart` запускает обои при старте сессии.
- `[background] type = "none"` отключает встроенный фон driftwm, чтобы layer-shell поверхность обоев была видна.

Бинарник после `make install` лежит в `$(PREFIX)/bin/driftwm-abstract-canvas`.

## 6. Важные моменты

- Трейт `WallpaperModule` требует `Send + Sync`.
- wgpu-конвейер (`pipeline`) и `bind_group` создаются лениво; при `resize` они сбрасываются в `None`.
- Чтение кадра с GPU на CPU (`renderer.copy_to_cpu`) — синхронная операция, подходит для обоев, но не для высокого FPS.
- Все шейдеры встроены через `include_str!` — меняя `.wgsl`, пересоберите проект (`cargo build`).
- Путь к IPC-сокету берётся из `WAYLAND_DISPLAY` или переменной `DRIFTWM_SOCKET`.
- При смене цветовой палитры цвета должны меняться плавно: модуль хранит как текущую, так и целевую палитру (`palette` / `target_palette`), а в `update` интерполирует между ними через `lerp_color` с накоплением `transition` от 0.0 до 1.0. Не меняйте палитру мгновенно — это создает резкий скачок цветов.

### 6.1. Ограничения Naga (компилятор WGSL в wgpu) — частые грабли

Ошибка валидации wgpu с сообщением `parsing error: ...` — это Naga не смог скомпилировать шейдер. Известные ловушки:

1. **Присваивание в swizzle запрещено.** `color.rgb = ...`, `color.rgb += ...`, `color.rgb *= ...` — невалидный lvalue для Naga. Собирайте цвет через выражения:
   ```wgsl
   // ПЛОХО:
   color.rgb = color.rgb + vec3<f32>(pulse);
   // ХОРОШО:
   let rgb = color.rgb + vec3<f32>(pulse);
   return vec4<f32>(rgb, color.a);
   ```
2. **Целочисленный суффикс — `i`/`u`, не `i32`/`u32`.** Литерал `5i32` Naga разбирает как `5i` + токен `32` → `expected ';', found "32"`. Правильно: `5i`, `16u`, или без суффикса с явным типом переменной: `var best_ring: i32 = 0;`.
3. **Рекурсия запрещена.** WGSL не поддерживает рекурсивные функции (`declaration of X is recursive`).
4. **`let` — иммутабельно.** Для присваиваний после объявления используйте `var x = ...; x = ...;`.
5. **Выравнивание uniform-буфера.** `array<vec4<f32>, N>` требует смещения, кратного 16 байтам. После скалярных полей вставьте Rust-padding: `_padding1: [u32; 3]` перед массивом и `_padding2` в конце, чтобы `size_of::<Uniforms>()` совпадал с WGSL-разметкой.

### 6.2. Копирование текстуры на CPU (`copy_to_cpu`)

- Текстура рендер-таргета обязана иметь usage `COPY_SRC` (помимо `RENDER_ATTACHMENT`).
- `bytes_per_row` в `TexelCopyBufferLayout` должен быть **кратен 256**:
  ```rust
  let row_pitch = ((width * 4 + 255) / 256) * 256;
  ```
  Staging-буфер — размером `row_pitch * height`; при чтении на CPU строки «разжимаются» (padding отбрасывается).
- Ошибки валидации draw-фазы не паникуют сразу — они откладываются до `encoder.finish()` (wgpu_core.rs:2678). Ошибки создания шейдера паникуют сразу (wgpu_core.rs:1084). По месту паники видно, в какой фазе искать проблему.

### 6.3. Соответствие шейдер ↔ Rust-код

- Если шейдер объявляет `@group(0) @binding(0) var<uniform>`, код обязан создать bind group layout с этой привязкой и pipeline layout с ним. Несоответствие не ловится при создании — оно всплывает как Validation Error на первом draw.
- После `ensure_pipeline` обязательно обновляйте uniforms (`update_uniforms`) до `begin_render_pass`, иначе буфер останется нулевым.

## 7. Стиль кода

- Комментарии на русском или английском — в зависимости от файла (проект смешанный).
- Используй `anyhow::Result` для ошибок, `tracing` для логов.
- Форматируй код через `cargo fmt` перед коммитом.

## 8. AI / ИИ

Этот проект был создан с использованием ИИ. Мы рекомендуем также использовать ИИ
при написании новых абстрактных модулей обоев для этого репозитория. Трейт
`WallpaperModule` и WGSL-шейдеры хорошо подходят для кода, сгенерированного ИИ,
а этот файл (`AGENTS.md`) содержит инструкции, помогающие ИИ-агентам писать
согласованные модули.
