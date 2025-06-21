import React from "react";
import { commands } from "@/bindings";
import { useTauriEvent } from "@/lib/hooks";
import useSWR from "swr";
import { unwrapResult } from "@/lib/result";

export default function GameScreen() {
    const profiles = unwrapResult(React.use(commands.getProfiles()));
    const { data: gameState, mutate } = useSWR(
        "fetch-game-state",
        async () => {
            return unwrapResult(await commands.getGameState());
        },
        {
            suspense: true,
            dedupingInterval: 100
        }
    );

    useTauriEvent("gameStateUpdate", () => {
        mutate();
    });

    const isSeeker = gameState.caught_state[gameState.my_id];

    const markCaught = async () => {
        if (!isSeeker) {
            unwrapResult(await commands.markCaught());
        }
    };

    const grabPowerup = async () => {
        if (gameState.available_powerup !== null) {
            unwrapResult(await commands.grabPowerup());
        }
    };

    const usePowerup = async () => {
        if (gameState.held_powerup !== null && gameState.held_powerup !== "PingSeeker") {
            unwrapResult(await commands.usePowerup());
        }
    };

    if (gameState.game_ended) {
        return <h2>Game Over! Syncing histories...</h2>;
    } else if (isSeeker && gameState.seekers_started === null) {
        return <h2>Waiting for hiders to hide...</h2>;
    } else {
        return (
            <>
                <h2>Hiders Left</h2>
                {Object.keys(gameState.caught_state)
                    .filter((k) => !gameState.caught_state[k])
                    .map((key) => (
                        <li key={key}>{profiles[key]?.display_name ?? key}</li>
                    ))}
                {!isSeeker && <button onClick={markCaught}>I got caught!</button>}
                <h2>Pings</h2>
                {gameState.last_global_ping !== null ? (
                    <>
                        <p>Last Ping: {gameState.last_global_ping}</p>
                        {Object.entries(gameState.pings)
                            .filter(([key, v]) => key && v !== undefined)
                            .map(([k, v]) => (
                                <li key={k}>
                                    {profiles[v!.display_player]?.display_name ?? v!.display_player}
                                    : {v && JSON.stringify(v.loc)}
                                </li>
                            ))}
                    </>
                ) : (
                    <small>Pings haven&apos;t started yet</small>
                )}
                <h2>Powerups</h2>
                {gameState.available_powerup && (
                    <p>
                        Powerup Available: {JSON.stringify(gameState.available_powerup)}{" "}
                        <button onClick={grabPowerup}>Grab!</button>
                    </p>
                )}
                {gameState.held_powerup && (
                    <p>
                        Held Powerup: {gameState.held_powerup}
                        {(gameState.held_powerup === "PingSeeker" && (
                            <small>(Will be used next ping)</small>
                        )) || <button onClick={usePowerup}>Use</button>}
                    </p>
                )}
            </>
        );
    }
}
