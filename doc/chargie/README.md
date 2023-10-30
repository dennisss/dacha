

"ChargieA GEv2"
64:Cf:D9:36:90:54

Use the service with UUID: 0xFFE1

- Send: "AT+STAT"
- Receive: "OK+STAT:0.00/5.02"

- Sent: "AT+CAPA?"
- Recieve: "OK+CAPA:32167"

- Send: "AT+AAUT0" or "AT+AAUT1"
    - Disable or enable Android auto mode

- Send: "AT+AUTO?"
- Receive: "OK+AUTO:0"


- Set Voltage limit:
    - SEND: "AT+VLTM" + X where X is the floating point volate value.
    - Set to 0 and disable the "AUTO" mode to disable voltage limiting?

- Send: "AT+HPWR?"
- Receive: "OK+HPWR:1.50"

- Send: "AT+LPWR?"
- Receive: "OK+HPWR:0.50"

- Send "AT+RETR" + a value
    - Sets the time between measurements of the power usage.

- "AT+VHYS?"
    - Voltage hysterisis value

- "AT+POWE" + N where N is an integer
    -  sets TX power level

- "AT+AFTC3FF"
    - Sets work mode?

- "AT+PIO20" or "AT+PIO21"
    - toggles the red led and toggles output power
    - Maybe also output power?

- "AT+PIO30" or "AT+PIO31"
    - 

- "AT+PWMT0"



"AT+PIO21"
"AT+PIO30"

boolean z = j.b(p1()).getBoolean("STOP_CHARGING_BT_DROP", false);
PrintStream printStream = System.out;
printStream.println("stopChargingBTdrop=" + z);
i.z("SEND_STRING", "value", z ? "AT+BEFC000" : "AT+BEFC3FF");
return;


## Reversing


        aVar.z("SEND_STRING", "value", "AT+HPWR" + this.t.q + "\u0000");
        com.ble.chargie.singletons.a aVar2 = this.s;
        aVar2.z("SEND_STRING", "value", "AT+LPWR" + this.t.r + "\u0000");
        com.ble.chargie.singletons.a aVar3 = this.s;
        aVar3.z("SEND_STRING", "value", "AT+RETR" + this.t.s);



        public void run() {
            ActivityHardwareFunctions.this.a0();
            if ((ActivityHardwareFunctions.this.t.l & 4) > 0 && ActivityHardwareFunctions.this.t.D.isEmpty()) {
                ActivityHardwareFunctions.this.s.z("SEND_STRING", "value", "AT+STAT");
            }
            ((TextView) ActivityHardwareFunctions.this.findViewById(2131296830)).setText(String.format("%.2fW (%.2fA/%.2fV)", new Object[]{Float.valueOf(ActivityHardwareFunctions.this.t.F), Float.valueOf(ActivityHardwareFunctions.this.t.G), Float.valueOf(ActivityHardwareFunctions.this.t.E)}));
            ActivityHardwareFunctions.this.v.postDelayed(this, 3000);
        }

    private class v extends BroadcastReceiver {
        private v() {
        }

        /* synthetic */ v(ActivityMain activityMain, k kVar) {
            this();
        }

        public void onReceive(Context context, Intent intent) {
            if (!ActivityMain.this.W.j(ChargeTracker.class)) {
                ActivityMain.this.V.c0 = (float) (intent.getIntExtra("temperature", 0) / 10);
                ActivityMain.this.V.d0 = ((float) intent.getIntExtra("voltage", -1)) / 1000.0f;
                int round = Math.round(((float) (intent.getIntExtra("level", -1) * 100)) / ((float) intent.getIntExtra("scale", -1)));
                if (round != ActivityMain.this.V.a0) {
                    ActivityMain.this.V.a0 = round;
                    ActivityMain.this.f1();
                }
            }
        }
    }


        /* renamed from: a  reason: collision with root package name */
    public static String f1799a = "00002902-0000-1000-8000-00805f9b34fb";

    /* renamed from: b  reason: collision with root package name */
    public static String f1800b = "0000ffe1-0000-1000-8000-00805f9b34fb";
    private static HashMap<String, String> c;

    static {
        HashMap<String, String> hashMap = new HashMap<>();
        c = hashMap;
        hashMap.put("0000ffe0-0000-1000-8000-00805f9b34fb", "ChargieFounder");
        c.put("0000ffd0-0000-1000-8000-00805f9b34fb", "ChargieFounder1");
        c.put("0000ffd1-0000-1000-8000-00805f9b34fb", "ChargieA");
        c.put("0000ffd2-0000-1000-8000-00805f9b34fb", "ChargieC");
        c.put("0000ffd3-0000-1000-8000-00805f9b34fb", "ChargieC100W");
        c.put("00001800-0000-1000-8000-00805f9b34fb", "Device Information Service");
        c.put(f1800b, "RX/TX data");
        c.put("00002a29-0000-1000-8000-00805f9b34fb", "Manufacturer Name String");
    }

            this.f1802b = context.getSharedPreferences("ChargiePrefs", 0);



                ActivityVoltageLimiter.this.s.z("SEND_STRING", "value", "AT+STAT");
        this.s.z("SEND_STRING", "value", "AT+VHYS?");
        this.s.z("SEND_STRING", "value", "AT+VLTM?");
        this.s.z("SEND_STRING", "value", "AT+VLTT?");


                aVar.z("SEND_STRING", "value", "AT+VLTM" + this.t.A);
        com.ble.chargie.singletons.a aVar2 = this.s;
        aVar2.z("SEND_STRING", "value", "AT+VLTT" + this.t.B);
        com.ble.chargie.singletons.a aVar3 = this.s;
        aVar3.z("SEND_STRING", "value", "AT+VHYS" + this.t.C);


                    sb.append("AT+AUTO");

            ActivityHardwareFunctions.this.s.z("SEND_STRING", "value", "AT+RESE");

            ActivityHardwareFunctions.this.s.z("SEND_STRING", "value", "AT+SERI?");


        aVar.z("SEND_STRING", "value", "AT+HPWR" + this.t.q + "\u0000");
        com.ble.chargie.singletons.a aVar2 = this.s;
        aVar2.z("SEND_STRING", "value", "AT+LPWR" + this.t.r + "\u0000");
        com.ble.chargie.singletons.a aVar3 = this.s;
        aVar3.z("SEND_STRING", "value", "AT+RETR" + this.t.s);



                this.s.z("SEND_STRING", "value", "AT+LPWR?");
        this.s.z("SEND_STRING", "value", "AT+HPWR?");
        this.s.z("SEND_STRING", "value", "AT+AUTO?");
        this.s.z("SEND_STRING", "value", "AT+RETR?");
        this.s.z("SEND_STRING", "value", "AT+AAUT?");