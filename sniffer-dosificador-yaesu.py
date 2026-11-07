# Librería estándar ----------------------------------------------------------------------------------------------------
#
import time
import re
import json
import sys
import datetime
# ----------------------------------------------------------------------------------------------------------------------

# Librerías PIP --------------------------------------------------------------------------------------------------------
#
import serial
import paho.mqtt.client as mqtt
# ----------------------------------------------------------------------------------------------------------------------


# Abrir e importar configuración ---------------------------------------------------------------------------------------
#
try:
    with open("config.json") as f:
        config = json.load(f)
    RX_PORT = config.get("rx_port", "COM1")
    RX_BAUD = config.get("rx_baud", 9600)
    TX_PORT = config.get("tx_port", "COM2")
    TX_BAUD = config.get("tx_baud", 9600)
    EX_DATA = config.get("ex_data", 1)
    TX_TIME = config.get("tx_time", 1)
    MQ_DATA = config.get("mq_data", True)
    MQ_HOST = config.get("mq_host", "localhost")
    MQ_PORT = config.get("mq_port", 1883)
    MQ_TOPIC = config.get("mq_topic", "default_topic")
    POLLING = config.get("polling", True)
    REPEATER = config.get("repeater", True)
    VERSION = "V20250731"
except Exception as e:
    print("No es posible abrir el fichero de configuración o el formato es incorrecto")
    print(e)
    input("Presione ENTER para salir...")
    sys.exit(0)
# ----------------------------------------------------------------------------------------------------------------------


# Inicialización serie TRX ---------------------------------------------------------------------------------------------
#
def do_serial_trx():
    try:
        sera = serial.Serial(port=RX_PORT)
        print(f'Configurando puerto TX/RX del TRX: {RX_PORT}')
        sera.baudrate = RX_BAUD
        print(f'Configurando velocidad TX/RX del TRX: {RX_BAUD}')
        sera.bytesize = serial.EIGHTBITS
        sera.parity = serial.PARITY_NONE
        sera.stopbits = serial.STOPBITS_TWO
        sera.setRTS = False
        sera.setDTR = False
        sera.rts = False
        sera.dtr = False
        sera.rtscts = False
        sera.dsrdtr = False
        sera.timeout = 1
        return sera
    except Exception as e:
        print("No es posible abrir el puerto")
        print(e)
        input("Presione ENTER para salir...")
        sys.exit(0)
# ----------------------------------------------------------------------------------------------------------------------


# Inicialización serie TX ----------------------------------------------------------------------------------------------
#
def do_serial_tx():
    try:
        serb = serial.Serial(port=TX_PORT)
        print(f'Configurando puerto TX del PA: {TX_PORT}')
        serb.baudrate = TX_BAUD
        print(f'Configurando velocidad TX del PA: {TX_BAUD}')
        serb.bytesize = serial.EIGHTBITS
        serb.parity = serial.PARITY_NONE
        serb.stopbits = serial.STOPBITS_TWO
        serb.setRTS = False
        serb.setDTR = False
        serb.rts = False
        serb.dtr = False
        serb.rtscts = False
        serb.dsrdtr = False
        return serb
    except Exception as e:
        print("No es posible abrir el puerto")
        print(e)
        input("Presione ENTER para salir...")
        sys.exit(0)
# ----------------------------------------------------------------------------------------------------------------------


# Conexión MQTT --------------------------------------------------------------------------------------------------------
#
def do_mqtt():
    try:
        print(f'Broker MQTT: Activado')
        client = mqtt.Client(mqtt.CallbackAPIVersion.VERSION2)
        client.connect_async(MQ_HOST, port=MQ_PORT, keepalive=60, bind_address="")
        print(f'Broker MQTT (host): {MQ_HOST}')
        print(f'Broker MQTT (puerto): {MQ_PORT}')
        print(f'Broker MQTT (topic): {MQ_TOPIC}')
        client.loop_start()
        time.sleep(0.3)
        mqtt_nro = 1
        while not client.is_connected():
            print(f'Broker MQTT (estado): Desconectado (intento: {mqtt_nro})')
            mqtt_nro += 1
            time.sleep(0.1)
            if mqtt_nro > 5:
                print("No es posible abrir la conexión MQTT")
                input("Presione ENTER para salir...")
                sys.exit(0)
        print(f'Broker MQTT (estado): Conectado')
        return client
    except Exception as e:
        print("No es posible abrir la conexión MQTT")
        print(e)
        input("Presione ENTER para salir...")
        sys.exit(0)
# ----------------------------------------------------------------------------------------------------------------------


# Generador de marca horaria -------------------------------------------------------------------------------------------
#
def to_ts(ts_epoch):
    try:
        ts = datetime.datetime.fromtimestamp(ts_epoch).strftime('%H:%M:%S')
        return ts
    except:
        return 'no time'
# ----------------------------------------------------------------------------------------------------------------------


# Colorear texto -------------------------------------------------------------------------------------------------------
#
def pinta_verde(str_data):
    return f'\033[32m{str_data}\033[0m'

def pinta_rojo(str_data):
    return f'\033[31m{str_data}\033[0m'

def pinta_azul(str_data):
    return f'\033[34m{str_data}\033[0m'

def pinta_naranja(str_data):
    return f'\033[38;2;255;165;0m{str_data}\033[0m'
# ----------------------------------------------------------------------------------------------------------------------

# Loop principal -------------------------------------------------------------------------------------------------------
#
def do_sniffer():
    try:
        # Se inicializa el puerto serie de la radio
        sera = do_serial_trx()

        # Se inicializa el puerto serie destino para repetir
        if REPEATER:
            serb = do_serial_tx()

        amp_data = ""
        fa_time = 0
        fa_last_send_time = time.time()
        fa_send_interval_time = TX_TIME
        fa_expire_time = EX_DATA
        polled = False
        print(f'Intervalo entre envíos: {int(fa_send_interval_time * 1000)} milisegundos')
        print(f'Tiempo de expiración de frecuencia: {int(fa_expire_time * 1000)} milisegundos')
        time.sleep(1)

        if MQ_DATA:
            mqtt_client = do_mqtt()
        else:
            print(f'Broker MQTT - Host: Desactivado')
        
        if not POLLING:
            print(f'{pinta_rojo(f'RXA | {to_ts(time.time())} | Esperando datos...')}')

        while True:
            # Sniffer of the PC<->Rig
            # Recepción de nuevos paquetes
            if sera.inWaiting() > 0:
                data_a = sera.read_until(b';')
                data_a = data_a.decode('utf-8')

                # Comprobación de datos válidos
                is_if = bool(re.search(r'IF\d{11}.*;', data_a))
                is_fa = bool(re.search(r'FA\d{8}.*;', data_a))

                # Si es un paquete válido lo procesamos
                if is_if:
                    qrg_data = re.findall(r'\d{11}', data_a)[0]
                    amp_data = "FA%s;" % qrg_data[3:]
                    fa_time = time.time()
                    print(f'{pinta_verde(f'RXA | {to_ts(fa_last_send_time)} | {amp_data}')}')
                    if MQ_DATA:
                        qrg = f"{int(''.join(c for c in amp_data if c.isdigit())) / 1000:.2f}"
                        mqtt_client.publish(f'{MQ_TOPIC}', qrg)
                    polled = False
                elif is_fa:
                    qrg_data = re.findall(r'\d{8}', data_a)[0]
                    amp_data = "FA%s;" % qrg_data
                    fa_time = time.time()
                    print(f'{pinta_verde(f'RXA | {to_ts(fa_last_send_time)} | {amp_data}')}')
                    if MQ_DATA:
                        qrg = f"{int(''.join(c for c in amp_data if c.isdigit())) / 1000:.2f}"
                        mqtt_client.publish(f'{MQ_TOPIC}', qrg)
                    polled = False

            # Calculamos si es necesario volver a enviar un dato o es necesario actualizarlo previamente
            fa_send = fa_last_send_time + fa_send_interval_time <= time.time()
            fa_expired = fa_time + fa_expire_time <= time.time()

            # Enviamos datos que si es necesario enviar
            if fa_send and REPEATER:
                # Si los datos existentes han caducado se solicitan nuevos
                if not fa_expired and bool(re.search(r'FA\d{8}.*;', amp_data)):
                    serb.write(amp_data.encode('utf-8'))
                    if MQ_DATA:
                        mqtt_client.publish(f'{MQ_TOPIC}/raw_if', amp_data)
                    print(f'{pinta_naranja(f'TXB | {to_ts(fa_last_send_time)} | {amp_data}')}')
                    fa_last_send_time = time.time()
                elif not polled and POLLING:
                    sera.write(b'FA;')
                    polled = True
                    print(f'{pinta_rojo(f'TXA | {to_ts(time.time())} | Polling...')}')
            time.sleep(0.05)


    except KeyboardInterrupt:
        print(f'\n** Salida por el usuario.')
        mqtt_client.disconnect()
        sys.exit(0)
    except EOFError:
        print(f'\n** Cerrando el programa debido a EOFError: {EOFError}')
        mqtt_client.disconnect()
        sys.exit(1)
    except OSError:
        print(f'\n** Cerrando el programa debido a OSError: {OSError}')
        mqtt_client.disconnect()
        sys.exit(1)
    except Exception as e:
        print(f'\n** Cerrando el programa debido a una excepción:')
        print(e)
        mqtt_client.disconnect()
        sys.exit(0)
# ----------------------------------------------------------------------------------------------------------------------

if __name__ == "__main__":
    print(f'Iniciando CAT-Bridge (Yaesu) {VERSION} by EB1TR')
    do_sniffer()