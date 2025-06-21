import React from "react";
import { commands, PlayerProfile } from "@/bindings";
import { unwrapResult } from "@/lib/result";

export default function SetupScreen() {
    const [displayName, setName] = React.useState("User");

    const onSave = async () => {
        const profile = { display_name: displayName, pfp_base64: null } as PlayerProfile;
        unwrapResult(await commands.completeSetup(profile));
    };

    return (
        <>
            <input
                name="displayName"
                value={displayName}
                placeholder="Display Name"
                onChange={(e) => setName(e.target.value)}
            />
            <button disabled={displayName === ""} onClick={onSave}>
                Save
            </button>
        </>
    );
}
