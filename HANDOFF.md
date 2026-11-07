# CAT Pacer - documento de continuidad

## Objetivo

CAT Pacer es una aplicacion Rust pequena y fiable que observa las respuestas
CAT de un Elecraft K3 y dosifica la frecuencia hacia un Expert 1K3FA.

No es un proxy CAT general: aprovecha las respuestas `IF` y `FA` producidas por
otra actividad CAT, y solo envia su propio polling `FA;` cuando la frecuencia
observada ha caducado.

## Principios acordados

- Un solo proceso, un solo hilo y un solo bucle de control.
- Estado minimo y memoria acotada.
- Sin `async`, frameworks, plugins ni logging complejo.
- Recuperacion automatica de fallos serie.
- Cada comportamiento importante debe estar documentado y probado.

## Implementacion

- Proyecto: `cat-pacer`.
- Lenguaje: Rust estable, edicion 2024.
- Entrada principal: `src/main.rs`.
- Configuracion local: `config.json`, ignorada por Git.
- Plantilla: `config.example.json`.
- Ejecutable: `target/release/cat-pacer.exe`.

El programa usa temporizadores monotonos, procesa CAT directamente como bytes,
limita el buffer RX a 4096 bytes, reintenta el polling una vez por segundo y
reabre los puertos desconectados cada dos segundos.

## Verificacion

Antes de entregar:

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

Las pruebas con hardware y la prueba continua de 24 horas estan descritas en
`TESTING.md`. No debe calificarse como mision critica hasta completarlas.

## Documentos

- `README.md`: instalacion, configuracion y uso.
- `PROTOCOL.md`: contrato CAT y temporizacion.
- `TESTING.md`: pruebas automaticas y con hardware.
- `HANDOFF.md`: este resumen de continuidad.

## Estado Git

La migracion desde Python esta preparada para commit. El repositorio remoto ya
se llama `EB1TR/cat-pacer`; la carpeta local se renombrara al finalizar para no
interrumpir las herramientas que trabajan sobre ella.
