import React from "react";
import { commands, PlayerProfile } from "@/bindings";

export default function SetupScreen() {
    const [displayName, setName] = React.useState("User");

    const onSave = async () => {
        const profile = { display_name: displayName, pfp_base64: null } as PlayerProfile;
        await commands.completeSetup(profile);
    };

    return (
        <>
            <div className="rounded-lg w-2/5 h-1/5 bg-white">
                <input
                    name="displayName"
                    value={displayName}
                    placeholder="Display Name"
                    onChange={(e) => setName(e.target.value)}
                />
                <button disabled={displayName === ""} onClick={onSave}>
                    Save
                </button>
            </div>
        </>
    );
}
