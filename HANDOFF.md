# CAT Pacer - documento de continuidad

## Objetivo

CAT Pacer es una aplicacion Rust pequena y fiable que observa las respuestas
CAT de un Elecraft K3 y dosifica la frecuencia hacia un Expert 1K3FA.

No es un proxy CAT general: aprovecha las respuestas `IF` y `FA` producidas por
otra actividad CAT, y solo envia su propio polling `FA;` cuando la frecuencia
observada ha caducado.

## Principios acordados

- Un solo bucle CAT; la interfaz de Windows tiene un hilo ligero independiente.
- Estado minimo y memoria acotada.
- Prioridad al hilo serie: buffer RX fijo, sin asignaciones por trama ni espera
  a la interfaz; compartir los textos de estado en lugar de copiarlos.
- Sin `async`, frameworks, plugins ni logging complejo.
- Recuperacion automatica de fallos serie.
- La comprobacion funcional la realiza manualmente el usuario.
- No añadir ni ejecutar pruebas automaticas salvo peticion expresa del usuario.

## Implementacion

- Proyecto: `cat-pacer`.
- Lenguaje: Rust estable, edicion 2024.
- Entrada principal: `src/main.rs`.
- Instancia unica de Windows: `src/instance.rs` (mutex nativo, solo al iniciar).
- Monitor y bandeja nativos de Windows: `src/ui.rs`.
- Configuracion local: `config.toml`, ignorada por Git.
- Plantilla comentada: `config.example.toml`.
- Ejecutable: `target/release/cat-pacer.exe`.

El programa usa temporizadores monotonos, procesa CAT directamente como bytes,
limita el buffer RX a 4096 bytes, reintenta el polling una vez por segundo y
reabre los puertos desconectados cada dos segundos.
La recepcion usa un array fijo de 4096 bytes con longitud y cursor de analisis.
Se examina cada byte una sola vez y solo se mueve la trama parcial restante al
final de cada lote. La marca monotona de la iteracion se reutiliza para las
tramas del lote y el refresco de la UI. El ultimo evento usa `Arc<str>` para
evitar copiar su texto al publicar el estado desde el hilo serie.

Los ejecutables Windows usan el subsistema grafico: abrirlos con doble clic
no crea una consola. En desarrollo se adjuntan a la terminal del proceso padre
si existe, y alli se gestiona Ctrl+C con una bandera atomica. Release omite las
escrituras rutinarias a terminal. Los errores de arranque se muestran en un
dialogo. Al salir, Rust libera los puertos y `main` devuelve el codigo `0`.

La ventana muestra frecuencia, conexiones, vigencia, ultimo envio y contadores.
La X oculta en bandeja y solicita una notificacion de ejecucion en segundo
plano; se ha retirado el boton de minimizar. El icono permite mostrar o salir. El hilo
de interfaz recibe instantaneas por una cola acotada a un elemento mediante
`try_send`, como maximo a 5 Hz: el bucle CAT no espera a la ventana ni al menu.
Se usa Win32 directamente con `windows-sys`, sin framework grafico. La ventana
usa tema Dracula con barra de titulo y cierre nativos, sin boton de minimizar;
Segoe UI en los textos y Consolas en la frecuencia. Los datos se agrupan en
frecuencia, conexiones, dosificacion, contadores y ultimo evento.
La ventana es redimensionable para FancyZones. `Layout::calculate` calcula la
distribucion en funcion del area cliente: dos columnas desde 600 unidades
logicas, apiladas por debajo y espaciado compacto si falta altura. Los mensajes
`WM_SIZE`, `WM_VSCROLL` y `WM_MOUSEWHEEL` reposicionan los controles en el hilo
de UI. Se conserva un tamaño minimo legible y aparece scroll vertical cuando
no cabe el contenido. Las fuentes se crean al arrancar y se reutilizan.
La UI separa la vigencia del dato del estado de polling. `poll_pending` indica
una consulta propia sin respuesta valida posterior; `received_after_poll` se
captura al recibir la siguiente trama valida, antes de limpiar `last_poll`.
No presentar un dato vigente como prueba de actividad CAT externa: el protocolo
no identifica que programa origino cada respuesta.

El icono se define una sola vez en `build.rs`, que genera el ICO multiresolucion
y lo incrusta con `winresource` (dependencia solo de compilacion). La ventana y
la bandeja cargan el recurso de icono `1` del propio ejecutable.

Se permite una sola instancia por sesion Windows mediante el mutex
`Local\EB1TR.CATPacer`, conservado hasta cerrar los puertos y la interfaz.
Una nueva ejecucion muestra la ventana existente y sale con codigo `0`, sin
cargar configuracion ni abrir puertos. Tambien detecta versiones anteriores
con ventana abierta que todavia no usen el mutex.

La configuracion usa TOML y se carga una vez al iniciar. Se conservan los mismos
nombres de parametros y unidades. Se ha sustituido `serde_json` por `toml`
con solo las funciones de lectura habilitadas. Se eliminaron los restos locales
de Python: `.venv/`, `build/`, `dist/` y el antiguo `config.json`. Se conservan
`config.toml` y las compilaciones Rust en `target/`.

El usuario quiere aprender Rust durante el desarrollo: explicar los cambios
con ejemplos pequenos del propio proyecto y equivalencias con Python.

## Verificacion manual

El usuario se encarga de las pruebas. A su peticion se retiraron los modulos de
pruebas Rust, los scripts automatizados, las configuraciones ficticias y sus
archivos temporales. `TESTING.md` queda unicamente como guia de comprobacion
manual con hardware.

## Documentos

- `README.md`: instalacion, configuracion y uso.
- `PROTOCOL.md`: contrato CAT y temporizacion.
- `TESTING.md`: guia de comprobaciones manuales con hardware.
- `HANDOFF.md`: este resumen de continuidad.

## Estado Git

CAT Pacer es la evolucion en Rust del CAT Bridge original en Python de EB1TR.
El Python original se conserva en el commit inicial `0806926`; no se mantiene
una copia legacy en el arbol de trabajo. La seccion "Origen y evolucion" de
`README.md` enlaza directamente a esa version historica.

La migracion desde Python se publico en el commit `4e079a6`. El repositorio
remoto se llama `EB1TR/cat-pacer`; la carpeta local sigue como `cat-bridge` al
estar abierta en el editor. El cambio de configuracion a TOML esta pendiente
de commit.
