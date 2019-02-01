#ifndef REDIS_RESP_H_
#define REDIS_RESP_H_

#include <vector>


enum RESPType {
	// These are virtual types just used to enumerate objects produced by the parser
	RESPTypeNil = '\x00',
	RESPTypeInline = '\x01',

	// These map directly to the type characters used in the protocol
	RESPTypeSimpleString = '+',
	RESPTypeError = '-',
	RESPTypeInteger = ':',
	RESPTypeBulkString = '$',
	RESPTypeArray = '*'
};

// Use to send a fast OK response
#define RESPOKConst ("+" "OK\r\n")

#define RESPNilConst ("$" "-1\r\n")

// Use this to predefine constant error messages that can be directly send over the wire
#define RESPErrorConst(s) ("-" s "\r\n")

// When using the above macros, use the following macro to determine the length to send
#define RESPConstLen(s) (sizeof(s) - 1)


// TODO: These will all need virtual destructors
struct RESPObject {
	virtual ~RESPObject() {}

	RESPType type;
};

struct RESPArray : RESPObject {
	~RESPArray() {
		for(int i = 0; i < items.size(); i++) {
			delete items[i];
		}
	}

	std::vector<RESPObject *> items;
};

// Used for SimpleString, Error, and BulkString
struct RESPBuffer : RESPObject {
	std::vector<char> str;
};

struct RESPInteger : RESPObject {
	int val;
};


enum RESPState {
	RESPStateType = 0,
	// Used for reading line terminated numbers 
	RESPStateLengthSign = 1,
	RESPStateLength = 2,
	RESPStateLengthEnd = 3,
	// Used for line terminated stirngs (SimpleString and Error)
	RESPStateString = 4,
	RESPStateStringEnd = 5,
	// Optimized cases for BulkString
	RESPStateData = 6,
	RESPStateDataEnd1 = 7, // \r
	RESPStateDataEnd2 = 8, // \n

	// Corresponding to accept/reject end states
	RESPStateDone = 9, // < NOTE: THis state is reset on calling grab()
	RESPStateError = 10
};

struct RESPParserStackEntry {
	RESPObject *obj;
	int arg; // Usually used to count the number of bytes or 
};

enum RESPParserStatus {
	RESPParserStatusIncomplete = 1,
	RESPParserStatusInvalid = 2,
	RESPParserStatusDone = 3
};

class RESPParser {
public:
	RESPParser() {
		state = RESPStateType;
		latest = NULL;
	}

	RESPParserStatus status() {
		if(state == RESPStateDone) {
			return RESPParserStatusDone;
		}
		else if(state == RESPStateError) {
			return RESPParserStatusInvalid;
		}
		else {
			return RESPParserStatusIncomplete;
		}
	}

	// Retries the last 
	// Must only be called once per parse 
	// NOTE: Only call this once the status is Done
	std::shared_ptr<RESPObject> grab() {

		// Patch fix for upgrading a single inline ping into a full array
		// TODO: This will also split the string by whitespace into separate bulk strings
		if(latest->type == RESPTypeInline) {
			latest->type = RESPTypeBulkString;

			auto newObj = new RESPArray();
			newObj->type = RESPTypeArray;
			newObj->items.push_back(latest); // The thing being that we must 
			latest = newObj;
		}


		auto ptr = std::shared_ptr<RESPObject>(latest);
		state = RESPStateType;
		latest = NULL;
		return ptr;
	}

	
	int parse(const char *buf, int len) {
		int i;
		char c;
		bool done = false;

		for(i = 0; i < len; i++) {
			c = buf[i];

			#define GOTO_DONE { state = RESPStateDone; return i + 1;  }
			#define GOTO_ERROR { state = RESPStateError; return i + 1;  }

			switch(state) {
				case RESPStateType: {
					RESPParserStackEntry ent;
					
					switch(c) {
						case RESPTypeError:
						case RESPTypeSimpleString:
							ent.obj = new RESPBuffer();
							state = RESPStateString;
							break;
						case RESPTypeInteger:
							ent.obj = new RESPInteger();
							state = RESPStateLengthSign;
							break;
						case RESPTypeArray:
							ent.obj = new RESPArray();
							state = RESPStateLengthSign;
							break;
						case RESPTypeBulkString:
							ent.obj = new RESPBuffer();
							state = RESPStateLengthSign;
							break;
						default:
							ent.obj = new RESPBuffer();
							state = RESPStateString;
							c = RESPTypeInline;
							i -= 1;
							break;
							//GOTO_ERROR
					};

					ent.obj->type = (RESPType) c;
					stack.push_back(ent);
					break;
				}
				case RESPStateLengthSign: {
					state = RESPStateLength;
					accum = 0;
					sign = 1;
					if(c == '-') {
						sign = -1;
						break;
					}

					// Otherwise fall through to Length
				}
				case RESPStateLength: {
					if(c == '\r') {
						state = RESPStateLengthEnd;
					}
					else {
						accum = (accum * 10) + (c - '0');
					}
					break;
				}
				case RESPStateLengthEnd: {
					if(c != '\n') {
						GOTO_ERROR
					}

					accum *= sign;


					RESPParserStackEntry &ent = stack[stack.size() - 1];

					switch(ent.obj->type) {
						case RESPTypeInteger:
							((RESPInteger *) ent.obj)->val = accum;
							done = true;
							break;
						case RESPTypeArray:
							if(accum < 0) {
								// TODO: For efficient destructors, we may want to distinguish between a Nil array and a nil bulk string
								ent.obj->type = RESPTypeNil; 
								done = true;
								break;
							}

							if(accum == 0) {
								done = true;
							}
							else {
								((RESPArray *) ent.obj)->items.resize(accum);
								ent.arg = 0; // First element
								state = RESPStateType;
							}
							break;
						case RESPTypeBulkString:
							if(accum < 0) {
								ent.obj->type = RESPTypeNil;
								done = true;
								break;
							}

							if(accum == 0) {
								state = RESPStateDataEnd1;
							}
							else {
								((RESPBuffer *) ent.obj)->str.resize(accum);
								ent.arg = 0; // First byte next
								state = RESPStateData;
							}
							break;
						default:
							// If we wrote the parser right, this should never happen
							GOTO_ERROR
					};

					break;
				}
				case RESPStateString: {
					if(c == '\r') {
						state = RESPStateStringEnd;
					}
					else {
						auto ent = stack[stack.size() - 1];
						((RESPBuffer *) ent.obj)->str.push_back(c);
					}

					break;
				}
				case RESPStateStringEnd: {
					if(c != '\n') {
						GOTO_ERROR
					}

					done = true;
					break;
				}
				case RESPStateData: {
					RESPParserStackEntry &ent = stack[stack.size() - 1];
					RESPBuffer *obj = (RESPBuffer *) ent.obj;

					int take = std::min(len - i, (int) obj->str.size() - ent.arg);
					memcpy(&obj->str[ent.arg], &buf[i], take);
					ent.arg += take;
					i += take - 1;

					if(ent.arg >= obj->str.size()) {
						state = RESPStateDataEnd1;
					}
					break;
				}
				case RESPStateDataEnd1: {
					if(c != '\r') {
						GOTO_ERROR
					}

					state = RESPStateDataEnd2;
					break;
				}
				case RESPStateDataEnd2: {
					if(c != '\n') {
						GOTO_ERROR
					}

					done = true;
					break;
				}

				// These final states are reset by some external actin
				case RESPStateDone:
				case RESPStateError: {
					return i;
				}
			}


			while(done) {
				done = false;

				RESPParserStackEntry ent = stack[stack.size() - 1];
				stack.pop_back();

				if(stack.size() == 0) {
					// We can give it return it as a valid 
					latest = ent.obj;
					GOTO_DONE
				}
				else { // Has an array parent
					RESPParserStackEntry &pent = stack[stack.size() - 1];
					RESPArray *arr = (RESPArray *) pent.obj;
					arr->items[pent.arg++] = ent.obj;
					if(pent.arg >= arr->items.size()) {
						done = true;
					}
					else {
						state = RESPStateType;
					}
				}
			}
		}

		return i;
	}



private:
	int sign; // Sign of accumalator below
	int accum; // Accumalator for length 

	RESPObject *latest;

	RESPState state;
	std::vector<RESPParserStackEntry> stack;

};


#endif