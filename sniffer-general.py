import serial
import time

ser = serial.Serial("COM4", timeout=None)  # open serial port
ser.baudrate = 38400
ser.bytesize = serial.EIGHTBITS
ser.parity = serial.PARITY_NONE
ser.stopbits = serial.STOPBITS_TWO
ser.setRTS = False
ser.setDTR = False
ser.rts = False
ser.dtr = False
ser.rtscts = False
ser.dsrdtr = False

ts = {}
tint = time.time()


def proc_rx(data):
    global tint
    try:
        trx = "%sms" % int((time.time() - ts[data[:3]]) * 1000)
    except:
        trx = "0ms"
    tdif = int((time.time() - tint) * 1000)
    tdif = "%sms" % int((time.time() - tint) * 1000)
    print("%s | %s | %s | %s" % (trx.rjust(6), tdif.rjust(6), data[:3].ljust(3), data))
    ts[data[:3]] = time.time()
    tint = time.time()


while ser.is_open:
    try:
        serial_bytes = ser.read_until(b';')
        serial_string = serial_bytes.decode('utf-8')
        proc_rx(serial_string)
    except Exception as e:
        print(e)
        pass
