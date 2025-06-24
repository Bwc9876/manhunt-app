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
        <div className="flex flex-col items-center justify-center p-3.5">
            <input
                className="text-center py-3 m-5 rounded-md border-2 border-gray-200"
                placeholder="Room Code"
                onChange={(e) => setRoomCode(e.target.value)}
            ></input>

            <button
                className="bg-blue-500 px-7 py-2 height text-white p-3.5 rounded-md"
                onClick={() => {
                    onStartGame(roomCode);
                }}
                disabled={roomCode.length === 0}
            >
                Join
            </button>
        </div>
    );
}
