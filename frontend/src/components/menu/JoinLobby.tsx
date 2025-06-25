import React, { useState } from "react";
import { commands, GameSettings } from "@/bindings";

export default function JoinLobby({ settings }: { settings: GameSettings }) {
    const [roomCode, setRoomCode] = useState("");

    const onStartGame = async (code: string | null) => {
        if (code) {
            try {
                const validCode = await commands.checkRoomCode(code);
                if (!validCode) {
                    window.alert("Invalid Join Code");
                    return;
                }
            } catch (e) {
                window.alert(`Failed to connect to Server ${e}`);
                return;
            }
        }

        await commands.startLobby(code, settings);
    };

    return (
        <div className="w-full h-full flex flex-col items-center justify-center">
            <div className="flex flex-col items-center justify-center p-3.5">
                <input
                    className="input-field py-3"
                    placeholder="Room Code"
                    onChange={(e) => setRoomCode(e.target.value)}
                ></input>

                <button
                    className="btn-blue px-7 py-3"
                    onClick={() => {
                        onStartGame(roomCode);
                    }}
                    disabled={roomCode.length === 0}
                >
                    Join
                </button>
            </div>
        </div>
    );
}
