package main

import (
	"fmt"
	"net"
	"io"
	"encoding/hex"
	
	"database/sql"
    _ "github.com/lib/pq"
)


const (
	CLIENT_LONG_PASSWORD = 0x01
	CLIENT_FOUND_ROWS = 0x02
	CLIENT_LONG_FLAG = 0x04
	CLIENT_CONNECT_WITH_DB = 0x08
	CLIENT_NO_SCHEMA = 0x10
	CLIENT_COMPRESS = 0x20
	CLIENT_ODBC = 0x40
	CLIENT_LOCAL_FILES = 0x80
	CLIENT_IGNORE_SPACE = 0x100
	CLIENT_PROTOCOL_41 = 0x200
	CLIENT_INTERACTIVE = 0x400
	CLIENT_SSL = 0x800
	CLIENT_IGNORE_SIGPIPE = 0x1000
	CLIENT_TRANSACTIONS = 0x2000
	CLIENT_RESERVED = 0x4000
	CLIENT_SECURE_CONNECTION = 0x8000
	
	CLIENT_MULTI_STATEMENTS = 0x10000
	CLIENT_MULTI_RESULTS = 0x00020000
	CLIENT_PS_MULTI_RESULTS = 0x00040000
	CLIENT_PLUGIN_AUTH = 0x00080000
	CLIENT_CONNECT_ATTRS = 0x00100000
	CLIENT_PLUGIN_AUTH_LENENC_CLIENT_DATA = 0x00200000
	CLIENT_CAN_HANDLE_EXPIRED_PASSWORDS = 0x00400000
	CLIENT_SESSION_TRACK = 0x00800000
	CLIENT_DEPRECATE_EOF = 0x01000000
	
)

// https://dev.mysql.com/doc/internals/en/character-set.html#packet-Protocol::CharacterSet
const (
	UTF8_GENERAL_CI = 33
)


// This will handle wrapping the data in a packet 
func writePacket(conn net.Conn, seq *int, data interface{}) {
	payload := StringVar(Marshal(data))
	bin := Marshal(&Packet{ Int3(len(payload)), Int1(*seq), payload })
	fmt.Println(hex.Dump(bin))
	conn.Write(bin)
	*seq += 1
}

func readPacket(conn net.Conn, seq *int, data *[]byte) error {
	buf := make([]byte, 512)
	n, err := conn.Read(buf) // TODO: If n has bytes left, then we got multiple packets
	if err != nil {
		return err
	}
	fmt.Println(hex.Dump(buf[0:n]))
	
	var pkt Packet
	UnmarshalPacket(buf, &pkt)
	
	if int(pkt.SequenceId) != *seq {
		fmt.Println("Received out of order packet (expected", seq, "got", pkt.SequenceId, ")")
	}
	
	*seq += 1
	
	*data = []byte(pkt.Payload)
	
	return nil
}

// Just sends a generic ok message
func sendOK(conn net.Conn, seq *int) {
	fmt.Println("Sending OK")
	ok := OKPacket{ 0, 0, 0, 2, 0, "" }
	writePacket(conn, seq, &ok)
}


func performInitialHandshake(conn net.Conn, id int) {
	
	seq := 0
	
	writePacket(conn, &seq, &Handshake{
		ProtocolVersion: 10,
		ServerVersion: "5.7.19",
		ConnectionId: Int4(id),
		AuthPluginDataPart1: "12345678", // len=8 // TODO
		Filler1: 0,
		CapabilityFlag1: (0xffff &^ (CLIENT_COMPRESS | CLIENT_SSL)),
		CharacterSet: UTF8_GENERAL_CI,
		StatusFlags: 2,
		CapabilityFlags2: ((CLIENT_DEPRECATE_EOF >> 16) | (49663 &^ (CLIENT_SESSION_TRACK >> 16))), // All // Int2((capabilities >> 16) & 0xffff), //,
		AuthPluginDataLen: 21, // The total data length should be 20 for the secure connection method
		Reserved: "\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00",
		AuthPluginDataPart2: "123456789abc\x00",
		AuthPluginName: "mysql_native_password", // TODO
	})
	
	var buf []byte
	readPacket(conn, &seq, &buf)
	
	var resp HandshakeResponse
	Unmarshal(buf, &resp)
	// TODO: Do something with the response

	sendOK(conn, &seq)
}




func handleConnection(conn net.Conn, id int) {
	// Making a companion connection to CockroachDB
	db, err := sql.Open("postgres", "postgresql://root@localhost:26257/wordpress?sslmode=disable")
	if err != nil {
		fmt.Println("error connecting to the database: ", err)
	}
	
	defer conn.Close()
	
	
	r, e := db.Query("USE wordpress")
	fmt.Println(r, e)
	
	
	performInitialHandshake(conn, id)

	//fmt.Println("Handshake:")
	//fmt.Println("----------")

	
	

	// At this point we have authenticated and are in command phase
	
	
	var buf []byte
	for {
		seq := 0
		err := readPacket(conn, &seq, &buf)
        if err != nil {
            if err != io.EOF {
                fmt.Println("read error:", err)
            }
            break
        }
				
		
		cmd := UnmarshalCommand(buf)
		
		switch cmd.(type) {
		case ComQuit:
			fmt.Println("Connection quiting...")
			break
		case ComInitDB:
			sendOK(conn, &seq)
		
		case ComQuery:
			c := cmd.(ComQuery)
			fmt.Println("Going to query:", c.Query)
			
			// TODO: Eventually send the entire response as a single network packet if possible
			
			//sendOK(conn, &seq)
			
			// Common functions: https://www.postgresql.org/docs/9.2/static/functions-info.html
			if c.Query == "select @@version_comment limit 1" {
				c.Query = "SELECT version()"
			}
			
			if c.Query == "SELECT DATABASE()" {
				c.Query = "SELECT current_database()"
				// TODO: Will also need to remap it in the output
			}
			
			
			writePacket(conn, &seq, &ResultSetHeader{ 1 })
			
			writePacket(conn, &seq, &ResultSetColumnDefinition{
				Catalog: "def",
				Schema: "",
				Table: "",
				OrgTable: "",
				Name: "Database",
				OrgName: "",
				FixedLength: 0x0c,
				CharacterSet: UTF8_GENERAL_CI,
				ColumnLength: 192,
				Type: MYSQL_TYPE_VAR_STRING,
				Flags: 0x01, // not null
				Decimals: 0,
				Filler: 0,
			});
			
			//writePacket(conn, &seq, &EOFPacket{ 0xfe, 0, 2 })
			
			
			rows, err := db.Query(string(c.Query))
			
			if err != nil {
				writePacket(conn, &seq, &ERRPacket{ 0xff, 0x0448, "#", "HY000", StringEOF(err.Error()) })
				continue
			}
			
			fmt.Println(rows)
			fmt.Println(err)
			fmt.Println("-------")
			fmt.Println(rows.Columns())
			//defer rows.Close()
			for rows.Next() {
				var str string
				if err := rows.Scan(&str); err != nil {
					//log.Fatal(err)
					fmt.Println("Error",err)
				}
				
				writePacket(conn, &seq, &ResultSetRow{ StringLenEnc(str) })
				//fmt.Printf("%d %d\n", id, balance)
			}
			rows.Close()
			
			//writePacket(conn, &seq, &ResultSetRow{ "wordpress" })
			
			writePacket(conn, &seq, &EOFPacket{ 0xfe, 0, 2 })
			//sendOK(conn, &seq)
			
		case ComFieldList:
			// Just send back that there are no fields
			writePacket(conn, &seq, &EOFPacket{ 0xfe, 0, 2 })
			
		}
		
	}
	
	
	fmt.Println("Connection closed")
}



func main() {
	
	/*
	str := "\x00\x01\x02\xff"
	fmt.Println([]byte(str))
	*/
	
	fmt.Println("Starting")
	
	listener, err := net.Listen("tcp", ":3306")
	if err != nil {
		fmt.Println("Failed", err)
		return
		// return errors.Wrap(err, "Unable to listen on " + listener.Addr().String() + "\n")
	}
	
	nconns := 0
	
	fmt.Println("Listening on", listener.Addr().String())
	for {
		// log.Println("Accept a connection request.")
		conn, err := listener.Accept()
		if err != nil {
			fmt.Println("Failed accepting a connection request:", err)
			continue
		}
		fmt.Println("Handle incoming messages.")
		nconns += 1
		
		go handleConnection(conn, nconns)
	}
	
	
	
	
	fmt.Printf("Exiting\n")
}
