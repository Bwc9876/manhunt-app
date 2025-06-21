import React from "react";
import { commands } from "@/bindings";
import { sharedSwrConfig, useTauriEvent } from "@/lib/hooks";
import useSWR from "swr";

export default function GameScreen() {
    const { data: profiles } = useSWR("game-get-profiles", commands.getProfiles);

    const { data: gameState, mutate } = useSWR(
        "fetch-game-state",
        commands.getGameState,
        sharedSwrConfig
    );

    useTauriEvent("gameStateUpdate", () => {
        mutate();
    });

    const isSeeker = gameState.caught_state[gameState.my_id];

    const markCaught = async () => {
        if (!isSeeker) {
            await commands.markCaught();
        }
    };

    const grabPowerup = async () => {
        if (gameState.available_powerup !== null) {
            await commands.grabPowerup();
        }
    };

    const activatePowerup = async () => {
        if (gameState.held_powerup !== null && gameState.held_powerup !== "PingSeeker") {
            await commands.activatePowerup();
        }
    };

    const quitToMenu = async () => {
        await commands.quitToMenu();
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
                        <li key={key}>{profiles?.[key]?.display_name ?? key}</li>
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
                                    {profiles?.[v!.display_player]?.display_name ??
                                        v!.display_player}
                                    : {v && JSON.stringify(v.loc)}
                                </li>
                            ))}
                    </>
                ) : (
                    <small>Pings haven&apos;t started yet</small>
                )}
                <h2>Powerups</h2>
                {gameState.last_powerup_spawn === null && (
                    <small>Powerups haven&apos;t started yet</small>
                )}
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
                        )) || <button onClick={activatePowerup}>Use</button>}
                    </p>
                )}
                <h2>Quit</h2>
                <button onClick={quitToMenu}>Quit To Menu</button>
            </>
        );
    }
}
