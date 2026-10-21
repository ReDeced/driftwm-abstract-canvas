# driftwm-abstract-canvas

**[English](README.md)** | Русский

Внешняя система обоев для [driftwm](https://github.com/roman/driftwm) через IPC.

Отдельное приложение для обоев, которое общается с driftwm через его IPC-сокет, предоставляя гибкие, GPU-ускоренные обои с живой синхронизацией камеры и зума.

## Возможности

- **IPC-интеграция**: Общается с driftwm через Unix-сокет для получения состояния камеры/зума в реальном времени
- **GPU-ускорение**: Использует wgpu для аппаратного ускорения рендеринга
- **Множество модулей**:
  - **Hyperbolic**: Мозаика {7,3} с анимированным панорамированием и настраиваемыми палитрами (отрицательная кривизна)
  - **Solid**: Простой сплошной цвет фона
  - **Image**: Обои из изображения с режимами подгонки (cover, contain, fill, center, tile)
  - **Spherical**: Обои с положительной (эллиптической) кривизной на сфере — клетки сходятся к полюсам, пространство замыкается на себя
- **Горячая перезагрузка**: Изменения конфигурации применяются без перезапуска
- **Плавные переходы цветов**: Цвета плавно перетекают друг в друга при смене палитры (без резких скачков)
- **Интеграция с темами**: Опциональная подписка на тему Noctalia для автоматического обновления палитры (отключена по умолчанию)
- **Layer Shell**: Рендерится как Wayland layer-shell поверхность на фоновом слое

## Установка

### Из исходного кода

```bash
cargo install --git https://github.com/ReDeced/driftwm-abstract-canvas
```

Или сборка локально:

```bash
git clone https://github.com/ReDeced/driftwm-abstract-canvas
cd driftwm-abstract-canvas
cargo build --release
./target/release/driftwm-abstract-canvas
```

## Конфигурация

Скопируйте `config.toml.example` в `~/.config/driftwm-abstract-canvas/config.toml` или `/etc/driftwm-abstract-canvas/config.toml` и настройте:

```toml
# Модуль обоев: "hyperbolic", "solid", "image", "spherical"
module = "hyperbolic"

[modules.hyperbolic]
pan_rate = 0.15      # Радиан в секунду
rmax = 3.5           # Максимальный радиус
palette = { type = "named", value = "noctalia" }
interactive = true

[modules.solid]
color = "#1a1a2e"

[modules.image]
path = "~/Pictures/wallpaper.jpg"
fit = "cover"

[modules.spherical]
radius = 1.0         # Радиус сферы (положительная кривизна)
pan_rate = 0.15      # Радиан в секунду
palette = { type = "named", value = "sphere" }
interactive = true

[layer]
namespace = "driftwm-abstract-canvas"
z_order = -100
output = ""
anchors = ["top", "bottom", "left", "right"]
exclusive_zone = 0

[render]
fps = 60
vsync = true
power_preference = "high"

[theme]
# Интеграция с темой Noctalia — опционально, выключена по умолчанию
enable = false
socket = ""
palette_key = "background"
```

## Использование

```bash
# Запуск с конфигурацией по умолчанию
driftwm-abstract-canvas

# Запуск с пользовательской конфигурацией
driftwm-abstract-canvas -c /путь/к/config.toml

# Включить отладочное логирование
driftwm-abstract-canvas --debug

# Вывести текущее состояние driftwm и выйти
driftwm-abstract-canvas --dump-state

# Переопределить модуль
driftwm-abstract-canvas -m solid
```

## Сферическое пространство (положительная кривизна)

Модуль `spherical` рисует мозаику на поверхности сферы с помощью ортографической проекции. В отличие от гиперболического диска (отрицательная кривизна, открытое пространство), сфера имеет **положительную кривизну**: пространство замкнуто, клетки сходятся к полюсам, а узор оборачивается вокруг себя.

### Шаблонный пример

Добавьте это в ваш `config.toml` для сферического обоя в стиле Noctalia:

```toml
module = "spherical"

[modules.spherical]
# Радиус сферы (положительная кривизна)
radius = 1.0
# Скорость анимации в радианах в секунду
pan_rate = 0.15
# Палитра: named ("default", "monochrome", "solarized", "sphere") или массив hex-цветов
palette = { type = "named", value = "sphere" }
# Включить параллакс при движении мыши
interactive = true

[layer]
namespace = "driftwm-abstract-canvas"
z_order = -100
```

Запуск:

```bash
driftwm-abstract-canvas -m spherical
```

Или переопределение модуля во время выполнения:

```bash
./target/release/driftwm-abstract-canvas -m spherical
```

## Интеграция с driftwm

Добавьте в конфигурацию driftwm (`~/.config/driftwm/config.toml` или аналогичный):

```toml
autostart = [
    "driftwm-abstract-canvas",
]

[background]
type = "none"
```

- `autostart` запускает обои при старте сессии driftwm.
- `[background] type = "none"` отключает встроенный фон driftwm, чтобы поверхность layer-shell обоев была видна.

## Архитектура

```
┌─────────────────┐     IPC (Unix socket)      ┌──────────────────┐
│   driftwm       │ ◄─────────────────────────► │ driftwm-abstract-canvas │
│  (композитор)   │   События состояния, команды│  (приложение обоев)│
└─────────────────┘                              └──────────────────┘
                                                        │
                                                        ▼
                                               ┌──────────────────┐
                                               │  wgpu Renderer   │
                                               │  (Vulkan/Metal/   │
                                               │   DX12/OpenGL)   │
                                               └──────────────────┘
                                                        │
                                                        ▼
                                               ┌──────────────────┐
                                               │ Wayland Layer    │
                                               │ Shell Surface    │
                                               │ (Background)     │
                                               └──────────────────┘
```

## Разработка модулей

Создавайте пользовательские модули обоев, реализуя трейт `WallpaperModule`:

```rust
pub trait WallpaperModule: Send + Sync {
    fn new(config: &HyperbolicConfig) -> Result<Self>;
    fn update(&mut self, frame: &FrameData);
    fn render(&mut self, renderer: &mut Renderer, frame: &FrameData) -> Result<TextureView>;
    fn resize(&mut self, width: u32, height: u32);
    fn reload_config(&mut self, modules: &ModuleConfigs) -> Result<()>;
    fn name(&self) -> &str;
}
```

## Лицензия

GPL-3.0-or-later - подробности в [LICENSE](LICENSE).

## Захват примеров для README

Чтобы добавить реальные скриншоты или GIF каждого модуля в этот README, запустите driftwm под Wayland (например, во вложенном композиторе, такого как `wayfire` или `weston`), затем выполните захват:

```bash
# Запуск driftwm-abstract-canvas с конкретным модулем
./target/release/driftwm-abstract-canvas -m hyperbolic
./target/release/driftwm-abstract-canvas -m spherical
./target/release/driftwm-abstract-canvas -m solid
./target/release/driftwm-abstract-canvas -m image -c путь/к/image.toml

# Захват скриншота (требуется grim/swaygrim или аналогичное)
grim -o <вывод> screenshot_<модуль>.png

# Захват короткого GIF/MP4 (требуется wf-recorder или аналогичное)
wf-recorder --output <вывод> -f demo_<модуль>.mp4 --duration 10
```

Поместите полученные файлы в `docs/screenshots/` и укажите их в README:

```markdown
![Модуль Hyperbolic](docs/screenshots/hyperbolic.png)
![Модуль Spherical](docs/screenshots/spherical.png)
```

Рекомендуемые настройки захвата:
- Разрешение: 1920×1080 (или ваш основной вывод)
- Длительность: 8–12 секунд на модуль, чтобы показать анимированное панорамирование
- Включите переход палитры: переключите `palette` в конфиге во время работы, чтобы продемонстрировать плавное затухание цветов

## Участие в разработке

1. Форкните репозиторий
2. Создайте ветку для новой функции
3. Внесите изменения
4. Запустите тесты: `cargo test`
5. Отправьте pull request

## AI / ИИ

Этот проект был создан с использованием ИИ. Мы также рекомендуем использовать
инструменты ИИ при создании новых абстрактных модулей обоев для этого репозитория
— трейт `WallpaperModule` и WGSL-шейдеры хорошо подходят для кода, сгенерированного
ИИ, а файл `AGENTS.md` содержит рекомендации, помогающие ИИ-агентам писать
согласованные модули.

## Связанные проекты

- [driftwm](https://github.com/roman/driftwm) - Композирующий оконный менеджер
- [Noctalia](https://github.com/roman/noctalia) - Система тем