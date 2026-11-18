export type Fixture = {
    width: number;
    height: number;
    bytesPerRow: number;
    data: Uint8Array | number[];
};
export type InitRequest = {
    type: "init";
};
export type EncodeRequestMsg = {
    type: "encode";
    symbology: string;
    text: string;
    module?: number;
    height?: number;
    ec?: number;
    mask?: number;
    version?: number;
    level?: number;
    columns?: number;
    rows?: number;
    cols?: number;
};
export type ReadyMsg = {
    type: "ready";
};
export type InitErrorMsg = {
    type: "error";
    message: string;
};
export type EncodedMsg = {
    type: "encoded";
    width: number;
    height: number;
    bytesPerRow: number;
    data: Uint8Array;
};
export type EncodeErrorMsg = {
    type: "encodeError";
    message: string;
};
export type EncodeReply = ReadyMsg | InitErrorMsg | EncodedMsg | EncodeErrorMsg;
