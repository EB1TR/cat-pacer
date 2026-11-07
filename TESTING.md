# Comprobaciones manuales

El usuario realiza estas comprobaciones con sus equipos. El proyecto no incluye
codigo ni scripts de pruebas automaticas.

## Pruebas con hardware

1. Iniciar con ambos equipos apagados y comprobar los reintentos sin consumo
   creciente de memoria o CPU.
2. Encender primero el K3 y despues el Expert; ambos deben reconectar sin
   reiniciar CAT Pacer.
3. Generar actividad CAT externa y comprobar que no aparece polling adicional.
4. Cambiar rapidamente de frecuencia y verificar que el Expert recibe como
   maximo un comando por cada intervalo `tx_time`.
5. Detener la actividad CAT y verificar `FA;` tras vencer `ex_data`.
6. Desconectar y reconectar cada cable serie por separado.
7. Enviar tramas parciales, concatenadas, desconocidas y con bytes corruptos.
8. Ejecutar durante al menos 24 horas observando CPU, memoria y continuidad.
9. En desarrollo, usando `cargo run`, detener con Ctrl+C tanto con puertos
   conectados como durante los reintentos. Debe aparecer `CAT Pacer detenido.`, devolver codigo `0` y
   permitir abrir nuevamente los puertos. Cargo no debe mostrar un error de
   terminacion del proceso.
10. Con datos reales, comprobar que la ventana muestra la frecuencia del K3,
    la ultima enviada al Expert, vigencia, conexiones y contadores.
11. Arrastrar la ventana y dejar abierto el menu de bandeja mientras se cambia
    la frecuencia: verificar que la cadencia hacia el Expert sigue siendo la
    configurada. Cerrar y restaurar la ventana no debe detener los envios.
12. Cerrar con la X: debe ocultarse en bandeja y solicitarse un aviso de que
    sigue ejecutandose en segundo plano. No debe aparecer boton de minimizar.
13. Ejecutar el programa de nuevo, tambien mezclando debug y release: debe
    mostrarse la ventana existente, sin iniciar otra conexion serie. Tras
    elegir Salir, debe poder iniciarse normalmente otra vez.
14. Ajustar a zonas anchas, estrechas y bajas de FancyZones. Comprobar que las
    conexiones cambian entre dos columnas y una, que todos los datos siguen
    accesibles con el scroll y que redimensionar no interrumpe la dosificacion.

## Criterios minimos

- Ningun bloqueo permanente tras perder una respuesta o un puerto.
- Sin crecimiento sostenido de memoria.
- Sin polling mientras llegan frecuencias validas dentro de `ex_data`.
- Cadencia hacia el Expert nunca superior a `tx_time`.
- Recuperacion automatica al restaurar cada puerto.

El uso de la expresion "mision critica" requiere completar estas pruebas con el
hardware real y documentar los resultados.
